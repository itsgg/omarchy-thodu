# 0007: Install, doctor and the unwanted paths

Status: review
Depends on: 0001, 0005
Spec: REQ-INSTALL-01, REQ-CLI-02, REQ-SESSION-04, REQ-SESSION-05
Goal: `thodu install` sets what Omarchy leaves off (the accessibility variables, the uinput rule, the plugin, the service, the keybindings GG chose) and `thodu doctor` reports each; a missing Hyprland, an empty window and an unregistered app each get the behavior section 8 gives them.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-INSTALL-01 REQ-CLI-02 REQ-SESSION-04 REQ-SESSION-05`
- GG runs `thodu install` once and `thodu doctor` reports everything present.

## Log

- 2026-10-21: created.
- 2026-10-22: implemented thodu install and thodu doctor (REQ-INSTALL-01), Hyprland connection verification (REQ-CLI-02), session opening on grid when no targets found (REQ-SESSION-04), and diagnostics for unregistered applications (REQ-SESSION-05). All acceptance tests pass.
- 2026-10-22: fixed the review findings for the udev rule, stale Hyprland sockets, doctor state, plugin assets and enablement, broken service links, Electron detection and CLI acceptance coverage. The acceptance command and full gate pass; cargo-deny is unavailable. Omarchy's key list showed SUPER+J is occupied, so the installer uses the free SUPER+SHIFT+J and SUPER+CTRL+J pair provisionally. GG's key choice remains open; no live install was run.
