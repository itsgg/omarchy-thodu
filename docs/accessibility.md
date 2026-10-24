# App accessibility setup

`thodu install` sets `ACCESSIBILITY_ENABLED=1` for the Omarchy session. `thodu doctor` checks that setting, but cannot change the environment of apps that are already running. Fully quit and relaunch apps after installation.

## Chromium and Chromium-based web apps

Close every Chromium window, including standalone app windows, then wait for all Chromium processes to exit before relaunching. Chromium app windows can reuse the existing browser process, so reopening only the web app may keep its old environment.

If a window has no AT-SPI targets, open `chrome://accessibility` to inspect or enable its tree for the current run. On Arch, `--force-renderer-accessibility` can be added on its own line in `~/.config/chromium-flags.conf`; fully quit and restart Chromium afterward. Prefer the normal session setup first because forced accessibility keeps more of the browser tree active. Scroll mode uses Thodu's uinput wheel path and does not need a browser extension.

References: [Chromium accessibility](https://www.chromium.org/developers/design-documents/accessibility/) and the [ArchWiki Chromium flags guide](https://wiki.archlinux.org/title/Chromium#Making_flags_persistent).

## Electron apps

Electron normally enables accessibility when it detects assistive technology. Quit and relaunch the app after installing Thodu. For an app started outside the Omarchy session, set the environment on launch, for example:

```sh
ACCESSIBILITY_ENABLED=1 obsidian
```

For a Flatpak app, set the variable for that app, then restart it:

```sh
flatpak override --user --env=ACCESSIBILITY_ENABLED=1 APP_ID
```

Replace `APP_ID` with the app's Flatpak ID. Remove the override with `flatpak override --user --unset-env=ACCESSIBILITY_ENABLED APP_ID`. Overrides are per-app and persist for the user. See [Electron accessibility](https://www.electronjs.org/docs/latest/tutorial/accessibility/) and the [Flatpak override reference](https://docs.flatpak.org/en/latest/flatpak-command-reference.html#flatpak-override).

## Website coverage

Thodu reads controls that Chromium and Electron expose through AT-SPI. A site may hide controls in custom widgets or expose incomplete accessibility data, so general website coverage remains limited. Screenshot hints can add visible candidates but may include text that is not interactive. Scroll mode falls back to the window when semantic panes are unavailable; it cannot identify nested page panes in that case. The planned Chromium extension is not included.
