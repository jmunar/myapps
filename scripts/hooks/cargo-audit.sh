#!/usr/bin/env bash
#
# Audit the dependency tree, run by the pre-push hook.
#
# Exits non-zero on a real advisory. A missing cargo-audit is not a failure:
# a developer tool that happens not to be installed must not block a push, and
# CI runs the same audit on every Cargo.toml/Cargo.lock change.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

if ! command -v cargo-audit >/dev/null 2>&1; then
    echo "cargo-audit is not installed; skipping the dependency audit." >&2
    echo "Install it with 'cargo install cargo-audit', or rely on the Security Audit workflow." >&2
    exit 0
fi

exec cargo audit
