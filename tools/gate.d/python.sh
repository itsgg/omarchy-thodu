# Python steps (uv, ruff, mypy, pytest with Hypothesis, pip-audit) in every root with a
# pyproject.toml. Sourced by tools/gate.

python_turn() {
    each_root pyproject.toml ruff-format uv run --frozen --quiet ruff format --check .
    each_root pyproject.toml ruff uv run --frozen --quiet ruff check .
    each_root pyproject.toml mypy uv run --frozen --quiet mypy .
    each_root pyproject.toml pytest uv run --frozen --quiet pytest
}

python_full() {
    each_root pyproject.toml uv-lock uv lock --check --quiet
    # The locked dependencies against the PyPI advisory database.
    each_root pyproject.toml pip-audit bash -c 'set -o pipefail; uv export --frozen --quiet --format requirements-txt --no-hashes \
        --no-emit-project | uvx --quiet pip-audit@2.10.1 --strict --progress-spinner off --no-deps --disable-pip -r /dev/stdin'
}

python_nightly() {
    each_root pyproject.toml pytest-nightly env HYPOTHESIS_PROFILE=nightly uv run --frozen --quiet pytest
}
