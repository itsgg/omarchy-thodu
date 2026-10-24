# Use Thodu

## Hints

The default hints session covers visible windows on the focused monitor and the Omarchy bar. Targets appear first; screenshot-based candidates arrive in a second pass. Press `Escape` to close. Thodu's bar entry is a hidden probe that reports existing bar controls for hint sessions; it does not show a Thodu status icon or settings menu.

- In search-first mode, lowercase letters filter target text and `Shift`+label keys select a target. Uppercase the final label key to activate it. `Return` activates the focused target.
- `Tab` and `Shift+Tab` change focus. `Space` opens the grid when no target is focused.
- `Shift+Return` right-clicks, `Control+Return` clicks with Control held, and two distinct `Return` presses within 300 ms double-click.
- In labels-only mode, type labels directly; `/` starts text search.
- Use `--scope focused` or `--window ADDRESS` to limit target collection.

## Grid and scroll

Grid mode selects a point on the focused monitor. Select a cell to refine it, then press `Return` to click its center.

Scroll mode selects panes across visible app windows. Hold `h`, `j`, `k` or `l` to repeat horizontal or vertical wheel input; a tap sends one action. `Shift` uses the larger configured step. `Tab` and `Shift+Tab` cycle areas, digits select an area, `Return` clears a numeric prefix, and `Escape` exits.

## Settings

Settings are read from `~/.config/thodu/config.toml` at the start of each session:

```toml
labels_only = false
status_display = "auto"
scroll_step = 1
scroll_shift_step = 5
```

`status_display` accepts `auto`, `always` or `off`. Auto hides routine collection counts after two seconds while warnings, search text, label input and the key cue remain visible. Scroll steps accept 1 through 100. Restart an active session after changing settings.

## Commands

```sh
thodu hints
thodu hints --scope focused
thodu grid
thodu scroll
thodu scroll --scope focused
thodu targets --json
thodu doctor
thodu bench --sessions 50
thodu record-tree --out tree.json
```

`thodu hints` and `thodu grid` also accept `--window ADDRESS` or `--window class:NAME`. `--test` draws an overlay without taking the keyboard; `--no-overlay` hides it. Benchmark data and Thodu state live under `~/.local/share/thodu`.
