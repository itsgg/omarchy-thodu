# 0008: Default install keybindings

Status: review
Depends on: none
Spec: REQ-INSTALL-01
Goal: a fresh `thodu install` uses the project's F12 keybindings by default, while preserving an existing local choice such as GG's Ctrl bindings and honoring explicit key overrides.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-INSTALL-01`
- On GG's session: a no-option install preserves the local Ctrl bindings, and `thodu doctor` reports them present.

## Log

- 2026-10-22: fresh non-interactive installs now use the F12 defaults, repeat installs preserve existing bindings, and explicit choices replace both. `tests/acceptance/reqs.sh REQ-INSTALL-01` and `tools/gate` passed. GG's local Ctrl bindings were applied and `thodu doctor` reports all items present.

