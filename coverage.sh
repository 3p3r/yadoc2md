#!/usr/bin/env bash
# Run unit tests with llvm coverage; fail if line coverage is below 90%.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

MIN="${MIN_COVERAGE:-90}"

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "installing cargo-llvm-cov..."
  cargo install cargo-llvm-cov --locked
fi

if ! rustup component list --installed 2>/dev/null | grep -q llvm-tools-preview; then
  echo "installing llvm-tools-preview..."
  rustup component add llvm-tools-preview
fi

export RUSTUP_AUTO_INSTALL=1

echo "==> running tests with coverage (minimum ${MIN}%)"
CARGO_LLVM_COV=1 cargo llvm-cov --lib --fail-under-lines "$MIN" --summary-only
cargo llvm-cov report --html --output-dir target/coverage/html
echo "==> html report: $ROOT/target/coverage/html/index.html"
