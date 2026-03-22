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

## 6. Frontend tests

If any commits on this branch (compared to `main`) touch frontend code (routes, handlers, HTML templates, or CSS classes used in assertions), run the **frontend-tester agent** (`.claude/agents/frontend-tester.md`) to generate or update integration tests for the changed routes. Commit any new or updated tests with message "Add/update frontend tests for [feature]". Include the co-author trailer. If no frontend code was changed, skip this step.

## 7. Run checks

Run `make check` (format, lint, and tests). If any check fails, fix the issue, commit the fix, and re-run until all checks pass. Stop and report to the user if a failure cannot be resolved automatically.

## 8. Push

Push the current branch to origin:
```
git push -u origin HEAD
```

## 9. Create PR

Create a pull request targeting `main` using `gh pr create`. The PR title **must** start with the ticket name derived from the branch name in square brackets. Extract the ticket prefix (everything up to and including the first number) from the branch name, uppercase it, and prepend it. For example, branch `feat-12-feature-xyz` → title starts with `[FEAT-12]`. Write a clear title after the prefix summarizing all changes on the branch. Use this format for the body:

```
## Summary
<bullet points summarizing what changed>

## Test plan
<how to verify the changes>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

Report the PR URL when done.
