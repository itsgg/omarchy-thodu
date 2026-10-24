#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

require_text() {
  local file="$1"
  local text="$2"
  if ! grep --fixed-strings --quiet -- "$text" "$file"; then
    printf 'ERROR: %s is missing required Thodu documentation: %s\n' "$file" "$text" >&2
    exit 1
  fi
}

require_text README.md '![Hints across visible windows](docs/screenshots/hints-overview.png)'
require_text README.md '![Scroll mode keeps the selected pane in view](docs/screenshots/scroll-mode.png)'
require_text docs/accessibility.md '## Chromium and Chromium-based web apps'
require_text docs/accessibility.md 'Close every Chromium window, including standalone app windows, then wait for all Chromium processes to exit before relaunching.'
require_text docs/accessibility.md 'chrome://accessibility'
require_text docs/accessibility.md '--force-renderer-accessibility'
require_text docs/accessibility.md 'ACCESSIBILITY_ENABLED=1 obsidian'
require_text docs/accessibility.md 'flatpak override --user --env=ACCESSIBILITY_ENABLED=1 APP_ID'
require_text docs/accessibility.md 'general website coverage remains limited'

for image in docs/screenshots/hints-overview.png docs/screenshots/scroll-mode.png; do
  if [[ ! -s "$image" ]]; then
    printf 'ERROR: README screenshot is missing or empty: %s\n' "$image" >&2
    exit 1
  fi
done

printf 'ok browser setup documentation and README screenshots\n'
