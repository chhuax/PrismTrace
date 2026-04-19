#!/usr/bin/env bash

set -euo pipefail

echo "==> cargo fmt --check"
cargo fmt --check

echo "==> cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets -- -D warnings

echo "==> cargo test --workspace"
cargo test --workspace

echo "==> cargo run -p prismtrace-host -- --discover"
cargo run -p prismtrace-host -- --discover
