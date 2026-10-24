#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

require_text() {
  local text="$1"
  if ! rg --fixed-strings --quiet -- "$text" README.md; then
    printf 'ERROR: README.md is missing browser setup guidance: %s\n' "$text" >&2
    exit 1
  fi
}

require_text '## Chromium, Electron and web apps'
require_text 'Close every Chromium window, including standalone app windows, then wait for all Chromium processes to exit before relaunching.'
require_text 'chrome://accessibility'
require_text '--force-renderer-accessibility'
require_text 'ACCESSIBILITY_ENABLED=1 obsidian'
require_text 'flatpak override --user --env=ACCESSIBILITY_ENABLED=1 APP_ID'
require_text 'general website coverage remains limited'
require_text '![Thodu labels the Open, Save and Close buttons](docs/screenshots/chromium-reference-hints.png)'

if [[ ! -s docs/screenshots/chromium-reference-hints.png ]]; then
  printf 'ERROR: Chromium reference screenshot is missing or empty\n' >&2
  exit 1
fi

printf 'ok browser setup documentation\n'
