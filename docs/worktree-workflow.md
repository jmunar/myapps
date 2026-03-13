# Worktree Workflow

Use git worktrees to work on multiple tickets simultaneously. Each worktree is
a full working copy on its own branch, sharing the same git history.

## Quick Reference

```bash
# Create a worktree for a new ticket (copies .env and data/ automatically)
./worktree.sh create <branch-name>

# List active worktrees
./worktree.sh list

# Remove a finished worktree (deletes merged branch automatically)
./worktree.sh remove <branch-name>
```

## Starting a Claude Session per Ticket

If you're using iTerm2, `worktree.sh create` automatically opens a new tab in
the worktree directory and launches `claude`. Otherwise, open a separate
terminal and start Claude Code manually:

```bash
cd ../myapps-<ticket>
claude
```

Each session is fully isolated — changes in one worktree don't affect others.

## Rust Compilation

Each worktree has its own `target/` directory, so the first build in a new
worktree starts with a cold cache for project crates.

**sccache** is configured in `.cargo/config.toml` to share compiled artifacts
(dependencies) across all worktrees. This means only your project crates
recompile from scratch — the dependency tree is cached globally.

To check sccache stats:

```bash
sccache --show-stats
```

## Disk Space

Each `target/` directory can be several hundred MB. Clean up finished worktrees
promptly:

```bash
git worktree remove ../myapps-<ticket>
# This removes the directory and its target/ along with it
```

## Tips

- Name worktrees by ticket/branch for clarity: `../myapps-fix-login`,
  `../myapps-add-categories`.
- Don't share `CARGO_TARGET_DIR` between worktrees — concurrent builds will
  conflict.
- The `.env` and `data/` files are not tracked by git — `worktree.sh create`
  copies them automatically.
