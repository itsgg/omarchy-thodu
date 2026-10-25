# Use Thodu

## Hints

The default hints session covers visible windows on the focused monitor and the Omarchy bar. Targets appear first; screenshot-based candidates arrive in a second pass. Press `Escape` to close. The Thodu bar control shows whether its service is ready, starts hints, grid, or scroll sessions, and opens the settings menu. The menu can change label mode, status detail and scroll steps; changes apply to new sessions. `Edit configuration` opens `~/.config/thodu/config.toml` in an Omarchy terminal editor. A hotkey can open the menu as it opens Omarchy's own panels: `omarchy-shell io.github.itsgg.thodu.menu toggle` (also `open` and `close`).

- Type a hint in lowercase to click its target at once. Type its final letter with `Shift` to select it instead; `Return` then clicks it. The final letter's case decides, so no click is sent while `Shift` is held.
- `Tab` and `Shift+Tab` change focus. `Space` opens the grid when no target is focused.
- `Shift+Return` right-clicks, `Control+Return` clicks with Control held, and two distinct `Return` presses within 300 ms double-click.
- `/` starts a text search; `Return` clicks the highlighted result. With `labels_only = false`, lowercase letters search text and hints need `Shift`.
- Use `--scope focused` or `--window ADDRESS` to limit target collection.

## Grid and scroll

Grid mode selects a point on the focused monitor. Type a cell's label to zoom into it; once the cells are small enough, a lowercase label clicks its cell's center, and `Shift` on its last letter selects the cell for `Return`, `Shift+Return` or a double `Return`. `Return` on a larger cell zooms into it. A click that switches workspace from the bar ends the session.

Scroll mode selects panes across visible app windows. Hold `h`, `j`, `k` or `l` to repeat horizontal or vertical wheel input; a tap sends one action. `Shift` uses the larger configured step. `Tab` and `Shift+Tab` cycle areas, digits select an area, `Return` clears a numeric prefix, and `Escape` exits.

## Settings

Settings are read from `~/.config/thodu/config.toml` at the start of each session:

```toml
labels_only = true
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
