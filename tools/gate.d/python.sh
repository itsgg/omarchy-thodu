# Python steps (uv, ruff, mypy, pytest with Hypothesis) in every root with a
# pyproject.toml. Sourced by tools/gate.

python_turn() {
    each_root pyproject.toml ruff-format uv run --frozen --quiet ruff format --check .
    each_root pyproject.toml ruff uv run --frozen --quiet ruff check .
    each_root pyproject.toml mypy uv run --frozen --quiet mypy .
    each_root pyproject.toml pytest uv run --frozen --quiet pytest
}

python_full() {
    each_root pyproject.toml uv-lock uv lock --check --quiet
}

python_nightly() {
    each_root pyproject.toml pytest-nightly env HYPOTHESIS_PROFILE=nightly uv run --frozen --quiet pytest
}
