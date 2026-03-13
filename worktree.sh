#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "$0")" && pwd)"

usage() {
    echo "Usage:"
    echo "  $(basename "$0") create <branch-name>   Create a worktree with .env and data/"
    echo "  $(basename "$0") remove <branch-name>   Remove a worktree"
    echo "  $(basename "$0") list                    List active worktrees"
    exit 1
}

cmd_create() {
    local branch="$1"
    local worktree_dir="$REPO_DIR/../myapps-$branch"

    if [ -d "$worktree_dir" ]; then
        echo "Error: $worktree_dir already exists"
        exit 1
    fi

    git -C "$REPO_DIR" worktree add "$worktree_dir" -b "$branch"

    # Copy .env if it exists
    if [ -f "$REPO_DIR/.env" ]; then
        cp "$REPO_DIR/.env" "$worktree_dir/.env"
        echo "Copied .env"
    else
        echo "Warning: no .env found in main repo"
    fi

    # Copy data/ (SQLite DBs)
    if [ -d "$REPO_DIR/data" ]; then
        cp -r "$REPO_DIR/data" "$worktree_dir/data"
        echo "Copied data/"
    else
        echo "Warning: no data/ directory found in main repo"
    fi

    echo ""
    echo "Worktree ready at: $worktree_dir"

    # If running in iTerm2, open a new tab in the worktree directory.
    # Uses a "Worktree" profile (if it exists) with "Applications in terminal
    # may change the title" disabled, so Claude cannot override the tab name.
    if [ "${TERM_PROGRAM:-}" = "iTerm.app" ]; then
        osascript <<EOF
tell application "iTerm2"
    tell current window
        try
            set newTab to (create tab with profile "Worktree")
        on error
            set newTab to (create tab with default profile)
        end try
        tell current session of newTab
            set name to "$branch"
            write text "cd $(printf '%q' "$worktree_dir") && claude"
        end tell
    end tell
end tell
EOF
        echo "Opened iTerm2 tab: $branch"
    else
        echo "  cd $worktree_dir"
    fi
}

cmd_remove() {
    local branch="$1"
    local worktree_dir="$REPO_DIR/../myapps-$branch"

    git -C "$REPO_DIR" worktree remove "$worktree_dir"
    echo "Removed worktree: $worktree_dir"

    # Delete the local branch only if the remote branch is already gone
    # (i.e. the PR was merged and the remote branch was deleted).
    # Use -D because squash-merged branches won't appear in --merged.
    git -C "$REPO_DIR" fetch --prune origin 2>/dev/null
    if git -C "$REPO_DIR" ls-remote --exit-code --heads origin "$branch" >/dev/null 2>&1; then
        echo "Branch '$branch' still exists on remote — kept local branch"
    else
        git -C "$REPO_DIR" branch -D "$branch" 2>/dev/null \
            && echo "Deleted branch: $branch" \
            || echo "Branch '$branch' not found or already deleted"
    fi
}

cmd_list() {
    git -C "$REPO_DIR" worktree list
}

[ $# -lt 1 ] && usage

case "$1" in
    create)
        [ $# -lt 2 ] && usage
        cmd_create "$2"
        ;;
    remove)
        [ $# -lt 2 ] && usage
        cmd_remove "$2"
        ;;
    list)
        cmd_list
        ;;
    *)
        usage
        ;;
esac
