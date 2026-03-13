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
    echo "  cd $worktree_dir"
}

cmd_remove() {
    local branch="$1"
    local worktree_dir="$REPO_DIR/../myapps-$branch"

    git -C "$REPO_DIR" worktree remove "$worktree_dir"
    echo "Removed worktree: $worktree_dir"

    # Delete the branch if it's been merged
    if git -C "$REPO_DIR" branch --merged main | grep -q "$branch"; then
        git -C "$REPO_DIR" branch -d "$branch"
        echo "Deleted merged branch: $branch"
    else
        echo "Branch '$branch' not yet merged into main — kept"
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
