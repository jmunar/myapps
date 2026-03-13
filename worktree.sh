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

    # Copy Claude settings.local.json (allowed permissions)
    local settings="$REPO_DIR/.claude/settings.local.json"
    if [ -f "$settings" ]; then
        mkdir -p "$worktree_dir/.claude"
        cp "$settings" "$worktree_dir/.claude/settings.local.json"
        echo "Copied .claude/settings.local.json"
    fi

    echo ""
    echo "Worktree ready at: $worktree_dir"

    # If running in iTerm2, open a new tab in the worktree directory.
    # Uses a "Worktree" profile (if it exists) configured so that the branch
    # name sticks as the tab title. Required profile settings:
    #   - General > Title: set to "Session Name"
    #   - General > "Applications in terminal may change the title": disabled
    if [ "${TERM_PROGRAM:-}" = "iTerm.app" ]; then
        local escaped_dir
        escaped_dir=$(printf '%q' "$worktree_dir")
        osascript <<APPLESCRIPT
tell application "iTerm2"
    tell current window
        try
            set newTab to (create tab with profile "Worktree")
        on error
            set newTab to (create tab with default profile)
        end try
        tell current session of newTab
            set name to "${branch}"
            write text "cd ${escaped_dir} && claude"
        end tell
    end tell
end tell
APPLESCRIPT
        echo "Opened iTerm2 tab: $branch"
    else
        echo "  cd $worktree_dir"
    fi
}

cmd_remove() {
    local branch="$1"
    local worktree_dir="$REPO_DIR/../myapps-$branch"

    # Fetch latest remote state and update the local main branch so that
    # branch deletion checks and future worktrees start from the latest code.
    git -C "$REPO_DIR" fetch --prune origin 2>/dev/null
    git -C "$REPO_DIR" pull --ff-only 2>/dev/null || true

    # Merge .claude/settings.local.json from worktree into main repo.
    # Combines the permissions.allow arrays (unique union) so any
    # permissions granted during development in the worktree are preserved.
    local wt_settings="$worktree_dir/.claude/settings.local.json"
    local main_settings="$REPO_DIR/.claude/settings.local.json"
    if [ -f "$wt_settings" ]; then
        if [ -f "$main_settings" ]; then
            local merged
            merged=$(jq -s '
                .[0] as $main | .[1] as $wt |
                $main * {permissions: {allow:
                    (($main.permissions.allow // []) + ($wt.permissions.allow // []))
                    | unique | sort
                }}
            ' "$main_settings" "$wt_settings")
            echo "$merged" > "$main_settings"
            echo "Merged .claude/settings.local.json"
        else
            mkdir -p "$REPO_DIR/.claude"
            cp "$wt_settings" "$main_settings"
            echo "Copied .claude/settings.local.json from worktree"
        fi
    fi

    git -C "$REPO_DIR" worktree remove "$worktree_dir"
    echo "Removed worktree: $worktree_dir"

    # Delete the local branch only if the remote branch is already gone
    # (i.e. the PR was merged and the remote branch was deleted).
    # Use -D because squash-merged branches won't appear in --merged.
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
