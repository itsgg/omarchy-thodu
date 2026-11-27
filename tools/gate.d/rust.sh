# Rust steps in every workspace root (rust-toolchain.toml pins the toolchain).
# cargo-nextest, cargo-deny and cargo-mutants are used when installed, and the
# gate says so when they are not. Sourced by tools/gate.

has_cargo() { cargo "$1" --version >/dev/null 2>&1; }

# Each checkout builds into a target directory of its own. Cargo names a path
# crate's build output by its path inside the workspace, so worktrees sharing
# one target directory (a target-dir in ~/.cargo/config.toml) run each other's
# build scripts. The directory is under ~/.cache, which /home snapshots skip,
# and named for the checkout, so a removed worktree's is easy to find. A
# caller that sets CARGO_TARGET_DIR keeps it (tools/land, for its scratch trees).
target_key=$(printf '%s' "$root" | cksum | cut -d' ' -f1)
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/cargo-target/$PROJECT-$(basename "$root")-$target_key}"

rust_turn() {
    each_root Cargo.toml cargo-fmt cargo fmt --all --check
    each_root Cargo.toml clippy cargo clippy --workspace --all-targets --locked --quiet -- -D warnings
    if has_cargo nextest; then
        each_root Cargo.toml cargo-test cargo nextest run --workspace --locked --no-tests=pass
    else
        each_root Cargo.toml cargo-test cargo test --workspace --locked --quiet
    fi
    # Tests that hold a wall-clock budget are marked #[ignore = "timing: run
    # alone"] and run here one at a time, so the parallel run's load cannot
    # make them miss it.
    each_root Cargo.toml cargo-test-timing cargo test --workspace --locked --quiet -- --ignored --test-threads=1
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
