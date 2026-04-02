Finish development on the current feature branch and open a PR.

Follow these steps in order. Stop and report if any step fails.

## 1. Verify branch state

- Run `git status` and `git log --oneline main..HEAD` to understand the current branch and what commits will be in the PR.
- If on `main`, stop and tell the user to switch to a feature branch first.

## 2. Commit pending work

If there are uncommitted changes (staged, unstaged, or untracked files relevant to the feature), commit them with an appropriate message describing the changes. Include the co-author trailer. If there are no uncommitted changes, skip this step.

## 3. Update documentation

Read all docs under `docs/` and the top-level `CLAUDE.md`. Based on the commits on this branch (compared to `main`), update any docs that are now outdated or incomplete. Common things to check:

- `docs/architecture.md` — project layout, routing structure, database schema, flow diagrams.
- `docs/requirements.md` — functional requirements, roadmap (move items from "Not yet implemented" to "Implemented" if applicable).
- `CLAUDE.md` — build commands, project conventions, documentation list.

Only make changes that are warranted by what was actually built. Do not invent features. If no docs need updating, skip this step.

## 4. Commit doc changes

If any docs were changed, create a commit with message "Update docs for [feature]" where [feature] is a short description derived from the branch name or recent commits. Include the co-author trailer.

## 5. Merge origin/main

Run:
```
git fetch origin
git merge origin/main
```

If there are merge conflicts, resolve them, then commit the merge. If the merge is clean, proceed.

## 6. Bump version

Determine the appropriate version bump by looking at the branch name prefix and commit messages on this branch (compared to `main`):

- If the branch name starts with `feat-` or any commit contains `[FEAT`: **minor** bump
- If any commit contains `[BREAKING`: **major** bump
- Otherwise (bug fixes, chores, refactors): **patch** bump

Run the corresponding Makefile target (`make bump-patch`, `make bump-minor`, or `make bump-major`). The Makefile only updates `Cargo.toml`; regenerate the lockfile with `cargo generate-lockfile`, then commit both:

```
cargo generate-lockfile
git add Cargo.toml Cargo.lock
git commit -m "Bump version to <new-version>"
```

Include the co-author trailer.

## 7. Frontend tests & screenshots

If any commits on this branch (compared to `main`) touch frontend code (routes, handlers, HTML templates, or CSS classes used in assertions):

1. **Integration tests**: Run the **frontend-tester agent** (`.claude/agents/frontend-tester.md`) to generate or update integration tests for the changed routes.

2. **Screenshots**: Review `scripts/screenshots.ts` and check whether the Playwright script needs updating for the changed app(s):
   - If a **new app** was added: add a new section to the Playwright script that navigates to the app's main pages and captures screenshots, following the existing pattern. Then add the corresponding `<img>` tags to the main `README.md` (under a new `### AppName` heading before the `---` separator) and to the app's own `crates/myapps-<app>/README.md`.
   - If **existing pages changed significantly** (new pages, layout overhauls, removed pages): update the Playwright script accordingly (add/remove/rename `snap()` calls) and update any affected `<img>` tags in `README.md` and the app README.
   - If the changes are minor (bug fixes, copy changes, small styling tweaks), the existing screenshots will be refreshed automatically when the script runs — no script changes needed.

   After any script changes, run `make screenshots` to regenerate the screenshot PNGs. Verify the new/updated images look correct.

3. **Commit**: Commit any new or updated tests, script changes, README updates, and regenerated screenshots with message "Add/update frontend tests and screenshots for [feature]". Include the co-author trailer.

If no frontend code was changed, skip this step.

## 8. Run checks

Run `make check` (format, lint, and tests). If any check fails, fix the issue, commit the fix, and re-run until all checks pass. Stop and report to the user if a failure cannot be resolved automatically.

## 9. Push

Push the current branch to origin:
```
git push -u origin HEAD
```

## 10. Create PR

Create a pull request targeting `main` using `gh pr create`. The PR title **must** start with the ticket name derived from the branch name in square brackets. Extract the ticket prefix (everything up to and including the first number) from the branch name, uppercase it, and prepend it. For example, branch `feat-12-feature-xyz` → title starts with `[FEAT-12]`. Write a clear title after the prefix summarizing all changes on the branch. Use this format for the body:

```
## Summary
<bullet points summarizing what changed>

## Test plan
<how to verify the changes>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

Report the PR URL when done.
