# Go steps in every root with a go.mod. govulncheck is used when installed.
# Sourced by tools/gate.

go_turn() {
    each_root go.mod gofmt sh -c 'out=$(gofmt -l .); [ -z "$out" ] || { echo "gofmt would change:"; echo "$out"; exit 1; }'
    each_root go.mod go-vet go vet ./...
    each_root go.mod go-test go test -shuffle=on ./...
}

go_full() {
    each_root go.mod go-test-race go test -race -shuffle=on ./...
    if command -v govulncheck >/dev/null; then
        each_root go.mod govulncheck govulncheck ./...
    else
        skip govulncheck "govulncheck is not installed"
    fi
}

go_nightly() {
    each_root go.mod go-test-repeat go test -count=10 -shuffle=on ./...
}
