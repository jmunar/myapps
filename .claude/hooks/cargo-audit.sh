#!/usr/bin/env bash
#
# PostToolUse hook: audit the dependency tree whenever it actually changes.
#
# Exit 2 is the only exit code that puts a message in front of Claude: on exit 0
# stdout goes to the debug log and nowhere else. The inline version this
# replaced ended in `2>/dev/null || true`, so it exited 0 whatever happened —
# a missing cargo-audit and a real advisory were equally invisible.
#
# Detection is by content hash rather than by inspecting the tool call. Matching
# the command string for `cargo update` and friends looks reasonable and is not:
# tool_input.command carries whole heredocs, so a commit message *about* cargo
# trips it, while an edit from any other source does not. The manifests
# themselves are the only honest signal.
set -uo pipefail

root="${CLAUDE_PROJECT_DIR:-$(pwd)}"

# Ask git for the git dir rather than assuming "$root/.git" is one: in a
# worktree it is a *file* pointing elsewhere, and writing the state file there
# would fail silently, leaving the hook to re-audit on every single tool call.
# --absolute-git-dir gives each worktree its own directory, which is what we
# want anyway since each has its own Cargo.lock.
gitdir="$(cd "$root" 2>/dev/null && git rev-parse --absolute-git-dir 2>/dev/null)"
state="${gitdir:-${TMPDIR:-/tmp}}/cargo-audit-hook.sha"

cat >/dev/null   # drain the payload; it is not needed

current="$(cat "$root/Cargo.toml" "$root/Cargo.lock" 2>/dev/null | sha256sum)"
[[ -z "$current" ]] && exit 0
[[ -f "$state" && "$(cat "$state")" == "$current" ]] && exit 0

# Record first: a failing audit should report once, not on every later call.
printf '%s' "$current" > "$state" 2>/dev/null

if ! command -v cargo-audit >/dev/null 2>&1; then
    echo "Cargo.toml/Cargo.lock changed, but cargo-audit is not installed, so the change went unaudited." >&2
    echo "Install it with 'cargo install cargo-audit', or let the Security Audit workflow catch it in CI." >&2
    exit 2
fi

if ! report="$(cd "$root" && cargo audit 2>&1)"; then
    echo "cargo audit failed on the dependency change that just landed:" >&2
    echo "$report" >&2
    exit 2
fi

exit 0
