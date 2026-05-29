#!/usr/bin/env bash
set -euo pipefail

if ! cargo fmt --check; then
  cat >&2 <<'EOF'
Stop hook: formatting check failed.
Run `cargo fmt` to apply Rust formatting, then rerun the checks before continuing.
EOF
  exit 1
fi

if ! cargo clippy --all-targets -- -D warnings; then
  cat >&2 <<'EOF'
Stop hook: clippy reported warnings or errors.
Fix the reported issues, then rerun `cargo clippy --all-targets -- -D warnings` before continuing.
EOF
  exit 1
fi
