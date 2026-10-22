# 0008: Default install keybindings

Status: done
Depends on: none
Spec: REQ-INSTALL-01
Goal: A fresh `thodu install` uses the project's F12 keybindings by default, preserves existing local bindings, and honors explicit key overrides.

Acceptance:
- run: `tests/acceptance/reqs.sh REQ-INSTALL-01`
- On GG's session: a no-option install preserves the local Ctrl bindings, and `thodu doctor` reports them present.

## Log

- 2026-10-22: Fresh installs now use the F12 pair, repeat installs preserve existing bindings, and explicit key choices replace both. The REQ-INSTALL-01 acceptance and local no-option install/doctor check passed.
