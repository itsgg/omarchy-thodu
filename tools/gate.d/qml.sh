# QML steps: Qt's qmllint with Omarchy's shell imports, the plugin manifest,
# and tests/qml/tst_*.qml under qmltestrunner with no display. CI runs the tests
# with stock Qt and skips only qmllint, which needs the machine's shell. Sourced
# by tools/gate.

# Qt's tools come from QT_BIN_DIR (default /usr/lib/qt6/bin), else PATH.
qt_bin() {
    local dir=${QT_BIN_DIR:-/usr/lib/qt6/bin}
    if [ -x "$dir/$1" ]; then echo "$dir/$1"; else command -v "$1"; fi
}

qml_lint() (
    linter=$(qt_bin qmllint) || { echo "qmllint not found"; exit 1; }
    shell=${OMARCHY_PATH:-/usr/share/omarchy}/shell
    imports=$(mktemp -d)
    trap 'rm -rf -- "$imports"' EXIT
    if [ -d "$shell" ]; then ln -s "$shell" "$imports/qs"; fi
    mapfile -t files < <(git ls-files --cached --others --exclude-standard -- '*.qml' | grep -v -E '^tests/' || true)
    if [ "${#files[@]}" -eq 0 ]; then echo "no QML files"; exit 0; fi
    status=0
    output=$("$linter" -I "$imports" "${files[@]}" 2>&1) || status=$?
    echo "$output"
    # qmllint exits 1 when it has warnings; anything else means it did not run.
    if [ "$status" -gt 1 ]; then
        echo "qmllint exited with status $status"
        exit 1
    fi
    # What makes a component refuse to load fails; the shell's injected
    # properties and singletons are the linter's blind spot, not findings.
    structural=$(grep -E '\[(property-override|duplicate|syntax|import|incompatible-type)\]|is used but it is not resolved \[unresolved-type\]' <<<"$output" || true)
    if [ -n "$structural" ]; then
        echo "structural findings:"
        echo "$structural"
        exit 1
    fi
)

qml_tests() {
    local runner
    if ! compgen -G "tests/qml/tst_*.qml" >/dev/null; then echo "no tests/qml/tst_*.qml"; return 0; fi
    runner=$(qt_bin qmltestrunner) || { echo "qmltestrunner not found"; return 1; }
    QT_QPA_PLATFORM=offscreen "$runner" -input tests/qml -import . -import tests/qml-stubs
}

qml_turn() {
    step manifest checks manifest
    if [ -n "${GATE_CI:-}" ]; then
        skip qmllint "qmllint needs Omarchy's shell imports; QML tests run with stock Qt"
        step qml-tests qml_tests
        return
    fi
    step qmllint qml_lint
    step qml-tests qml_tests
}

qml_full() {
    :
}

qml_nightly() {
    :
}
