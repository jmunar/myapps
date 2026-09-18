Finish development on the current feature branch and open a PR.

Work through these in order. Stop and report if a step fails in a way you
can't resolve.

## 1. Check the branch

`git status` and `git log --oneline main..HEAD`. If you're on `main`, stop and
say so.

## 2. Commit what's outstanding

Commit any uncommitted work belonging to the feature, with the co-author
trailer.

## 3. Update the docs

Read `docs/` and `CLAUDE.md` against what the branch actually changed, and
update only what is now wrong or missing. Do not invent features.

- `docs/architecture.md` — layout, routing, schema, diagrams.
- `docs/requirements.md` — move roadmap items to "Implemented" when they are.
- `docs/deployment.md` — any new env var, deploy step or server-side
  requirement. A new env var means five files; CLAUDE.md lists them.
- `CLAUDE.md` — only when the branch adds or invalidates a *gotcha*. It is a
  list of traps, not an inventory: a new route, app or command does not belong
  there on its own.

Commit as "Update docs for [feature]" if anything changed.

## 4. Merge main

```
git fetch origin && git merge origin/main
```

Resolve any conflicts and commit the merge.

## 5. Bump the version

CD fails the release if the version isn't higher than the latest tag. Pick the
bump from the branch name and commit messages: `[BREAKING` → major, `feat-` or
`[FEAT` → minor, otherwise patch. Then:

```
make bump-<type>
cargo update --workspace
git add Cargo.toml Cargo.lock && git commit -m "Bump version to <new-version>"
```

`cargo update --workspace` touches only the workspace member's own entry. Don't
use `cargo generate-lockfile` here — it re-resolves the whole graph and drags
unrelated dependency bumps into the PR, which is Dependabot's job.

## 6. Frontend work

If the branch touched routes, handlers, templates or CSS:

- Run the **frontend-tester agent** (`.claude/agents/frontend-tester.md`) for
  the changed routes.
- Screenshots: a new app needs a section in `scripts/screenshots.ts` and
  `<img>` tags in the root and app `README.md`; significant page changes need
  `snap()` calls added, removed or renamed; minor changes need neither, since
  the existing shots are regenerated anyway. Run `make screenshots` after any
  script change and look at the results.
- Commit tests, script and screenshots together.

## 7. Check, push, PR

`make check` until it passes, then `git push -u origin HEAD`.

Open the PR against `main` with `gh pr create`. The title must start with the
ticket from the branch name in brackets — `feat-12-feature-xyz` → `[FEAT-12]` —
followed by a summary of the whole branch. Body:

```
## Summary
<what changed, in bullets>

## Test plan
<how to verify it>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

Report the PR URL.
