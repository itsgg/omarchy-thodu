# 0008: Default install keybindings

Status: proposed
Why: SPEC open question O4 says no keys are bound by default and GG chooses at install. GG decided that a fresh install should default to `SUPER + SHIFT + F12` for hints and `SUPER + CTRL + F12` for the grid. His own Hyprland configuration uses `CTRL + SPACE` and `CTRL + SHIFT + SPACE`; a repeat install without key options must preserve those local choices. The existing `--hints-key` and `--grid-key` options remain explicit overrides. On merge, resolve O4 with the F12 pair as the project default and GG's Ctrl pair as his personal override.

## Modified

### REQ-INSTALL-01: Works on Omarchy as installed
When `thodu install` runs, the system shall set `ACCESSIBILITY_ENABLED=1` and `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1` in the Hyprland session environment, install a udev rule giving the logged-in user `/dev/uinput` through `uaccess`, install and enable the overlay plugin and the `thodud` user service, and bind the keys in the user's Lua configuration; `thodu doctor` shall report each of these as present or missing. On a fresh configuration, an install without key options shall bind `SUPER + SHIFT + F12` to `thodu hints` and `SUPER + CTRL + F12` to `thodu grid`. If both Thodu bindings already exist, an install without key options shall preserve them. Supplying both `--hints-key` and `--grid-key` shall replace both Thodu bindings with those choices; supplying only one option shall remain an error.
Acceptance:
- scenario: on a fake home, `thodu install` with no existing Thodu bindings writes the F12 defaults, and `thodu doctor` reports every item present.
- scenario: on a fake home with both Thodu bindings, install without key options preserves them.
- scenario: explicit key options replace both Thodu bindings.

## Removed

None.

## Tests

- Add acceptance tests named for REQ-INSTALL-01 for fresh-install defaults and preservation of existing bindings. Keep coverage that explicit key options replace both bindings.
