#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
data="${XDG_DATA_HOME:-$HOME/.local/share}/thodu"
mkdir -p "$data"
path_dir="$(mktemp -d "$data/ci-path.XXXXXX")"
trap 'rm -rf "$path_dir"' EXIT

for tool in env bash grep dirname; do
  ln -s "$(command -v "$tool")" "$path_dir/$tool"
done

if PATH="$path_dir" command -v rg >/dev/null 2>&1; then
  printf 'ERROR: restricted PATH unexpectedly contains rg\n' >&2
  exit 1
fi

PATH="$path_dir" "$root/tests/acceptance/docs-browser-setup.sh"
