# Thodu

Thodu is a keyboard hint picker for Omarchy. It labels clickable controls in the focused window and the Omarchy bar, so you can choose them without reaching for the pointer. Thodu means "touch" in Tamil.

This is an early, Omarchy-only milestone. It uses accessibility trees for GTK 3, GTK 4, Qt Widgets, Chromium controls and Electron controls, plus clickable items in the Omarchy bar. When a visible list row is missing from an accessibility tree, a bounded screenshot detector can add click hints for its text and box-like controls. Those image hints have no searchable text and use an amber style. They fill some gaps; they do not recognize every icon or guarantee that every visible control is labeled. A grid remains available for choosing any point on the focused monitor. Web page content, scrolling, dragging and hovering are not supported yet.

## Install

Requirements:

- Omarchy with Hyprland using the Lua configuration.
- `grim`, for the screenshot fallback when an accessibility tree is incomplete.
- The Rust toolchain pinned in `rust-toolchain.toml` to build from source.
- An active graphical session. Installation configures accessibility, the virtual input device rule, the Omarchy plugin, a user service and Hyprland bindings.

Build and install the executables side by side. The `thodu install` command uses the `thodud` executable beside it:

```sh
cargo build --release
install -Dm755 target/release/thodu "$HOME/.local/bin/thodu"
install -Dm755 target/release/thodud "$HOME/.local/bin/thodud"
thodu install
thodu doctor
```

The first install with no existing Thodu bindings uses `SUPER + SHIFT + F12` for hints and `SUPER + CTRL + F12` for the grid. Re-running the installer preserves existing Thodu bindings. To set or replace both bindings explicitly, pass both options:

```sh
thodu install \
  --hints-key "SUPER + CTRL + SPACE" \
  --grid-key "SUPER + CTRL + SHIFT + SPACE"
```

Installation may ask for authorization to add the `/dev/uinput` rule. `thodu doctor` reports which installation requirements are present. If the Omarchy shell was not restarted by the installer, run `omarchy restart shell` and then `thodu doctor` again.

## Use

- Press the hints binding to show labels for targets in the focused window and the bar on its monitor.
- In the default search-first mode, type text to narrow targets. Use `Shift` with a label letter to enter a label. In labels-only mode, type a label directly; press `/` to search by text.
- Completing a label in lowercase focuses it. Making the label's final letter uppercase activates it immediately. `Return` activates the focused target.
- `Tab` and `Shift+Tab` move focus. `Escape` closes the session. `Space` opens the grid when no target is focused.
- `Shift+Return` right-clicks, `Control+Return` clicks with Control held, and two distinct `Return` presses within 300 ms double-click.
- Press the grid binding to choose a point on the focused monitor. Select a cell to refine it, then press `Return` to click its center.

The configuration file is `~/.config/thodu/config.toml`. The supported setting is:

```toml
labels_only = true
```

The default is `false`, which starts in search-first mode. Restart an active session after changing the setting.

## Commands

```sh
thodu hints                  # show target labels
thodu grid                   # choose a point with the grid
thodu doctor                 # check the installation
thodu bench --sessions 50    # measure time until labels are drawn
thodu record-tree --out tree.json  # record the focused app accessibility tree
```

`thodu hints` and `thodu grid` accept `--window ADDRESS` to target a Hyprland window address or `class:NAME`. The `--test` option shows an overlay without taking the keyboard, and `--no-overlay` runs a session without showing it. Benchmark samples are stored under `~/.local/share/thodu/bench/`.

## Development

```sh
cargo test
tools/gate
```

`tools/gate` runs the full project gate, including Rust, QML and acceptance checks. `make install` copies only the Omarchy plugin files; `make reload` copies them and restarts the Omarchy shell. Use `thodu install` to configure the service, plugin, accessibility environment, input rule and keybindings for the installed executables.

The project contract is in [SEED.md](SEED.md) and [SPEC.md](SPEC.md). Work items and their acceptance checks are in [tasks/](tasks/). The repository workflow is documented in [AGENTS.md](AGENTS.md).

The screenshot fallback uses the [Woints](https://github.com/wochap/woints) classical detector, pinned to a source revision in `Cargo.toml`. Woints is MIT licensed.
