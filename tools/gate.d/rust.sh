# Rust steps in every workspace root (rust-toolchain.toml pins the toolchain).
# cargo-nextest, cargo-deny and cargo-mutants are used when installed, and the
# gate says so when they are not. Sourced by tools/gate.

has_cargo() { cargo "$1" --version >/dev/null 2>&1; }

rust_turn() {
    each_root Cargo.toml cargo-fmt cargo fmt --all --check
    each_root Cargo.toml clippy cargo clippy --workspace --all-targets --locked --quiet -- -D warnings
    if has_cargo nextest; then
        each_root Cargo.toml cargo-test cargo nextest run --workspace --locked --no-tests=pass
    else
        each_root Cargo.toml cargo-test cargo test --workspace --locked --quiet
    fi
}

rust_full() {
    each_root Cargo.toml cargo-build cargo build --workspace --release --locked --quiet
    if has_cargo deny; then
        each_root deny.toml cargo-deny cargo deny check
    else
        skip cargo-deny "cargo-deny is not installed"
    fi
}

rust_nightly() {
    if has_cargo mutants; then
        each_root Cargo.toml cargo-mutants cargo mutants --workspace
    else
        skip cargo-mutants "cargo-mutants is not installed"
    fi
}
