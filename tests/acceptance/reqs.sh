#!/bin/sh
# Acceptance of record: tests/acceptance/reqs.sh REQ-AREA-NN... checks that each
# requirement has a test named for it (its ID in snake case, as in
# req_act_01_clicks_the_center, in Rust or in tests/qml/tst_*.qml), and that
# every Rust and QML test passes. CI checks QML test names but skips execution,
# because its runner does not have Omarchy's Qt and shell.
set -eu
cd "$(dirname "$0")/../.."
[ $# -gt 0 ] || { echo "ERROR name at least one requirement"; exit 2; }
[ -f Cargo.toml ] || { echo "ERROR there is no Cargo workspace yet"; exit 1; }
names=$(cargo test --workspace --locked --quiet -- --list 2>/dev/null | sed -n 's/: test$//p')
qml=""
for f in tests/qml/tst_*.qml; do
    [ -f "$f" ] && qml="$qml$(grep -o 'function test_[a-z0-9_]*' "$f" | sed 's/function //')
"
done
for req in "$@"; do
    id=$(echo "$req" | tr 'A-Z-' 'a-z_')
    printf '%s\n%s\n' "$names" "$qml" | grep -q "$id" || { echo "ERROR no test is named for $req ($id)"; exit 1; }
done
cargo test --workspace --locked --quiet
if [ -n "$qml" ]; then
    if [ -n "${GATE_CI:-}" ]; then
        echo "skip QML acceptance: Omarchy Qt and shell are unavailable in CI"
    else
        QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input tests/qml -import .
    fi
fi
echo "ok: $*"
