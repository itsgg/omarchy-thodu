# Install Thodu

Requirements: Omarchy with Hyprland and Lua configuration, `grim`, Python 3, the Rust toolchain in `rust-toolchain.toml`, and an active graphical session.

Build and install both executables into the same directory. `thodu install` starts the user service and configures the plugin, accessibility environment, virtual input rule and default keybindings.

```sh
set -eu
cargo build --release
target_dir="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
: "${target_dir:?ERROR could not determine Cargo target directory}"
install -Dm755 "$target_dir/release/thodu" "$HOME/.local/bin/thodu"
install -Dm755 "$target_dir/release/thodud" "$HOME/.local/bin/thodud"
thodu install
thodu doctor
```

The first install uses `Super+Shift+F12` for hints, `Super+Ctrl+F12` for grid and `Super+Alt+F12` for scroll mode. The installer preserves existing hints and grid keys. Change only the scroll key with `--scroll-key`; pass both `--hints-key` and `--grid-key` to replace those bindings.

```sh
thodu install --scroll-key "CTRL + ALT + SPACE"
```

The installer may ask to add the `/dev/uinput` rule. `thodu doctor` reports missing setup. If the shell did not restart automatically, run `omarchy restart shell` and then `thodu doctor` again.
