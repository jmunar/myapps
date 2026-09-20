# MyApps

A single Rust binary serving several small personal web apps behind one login:
LeanFin (expenses), MindFlow (thoughts/mind map), VoiceToText (transcription),
FormInput (custom forms), Notes (markdown), FileClipboard (file transfer between
a user's devices). Axum + HTMX + server-rendered HTML + SQLite via sqlx with
runtime-checked queries (no compile-time macros). Deployed to an Odroid N2
(aarch64, 4 GB RAM) behind nginx.

Each app is a crate under `crates/` implementing the `App` trait from
`myapps_core::registry`; `crates/myapps-core/` holds everything shared (auth,
config, db, layout, i18n, components, command bar, services). Apps never depend
on each other. `src/main.rs` just registers them and delegates to
`myapps_core::cli`.

`cargo run -- --help` lists the CLI subcommands; the `Makefile` lists the build,
deploy and version targets. `make check` is exactly what CI runs.

## Gotchas

These are the things that have actually broken, and that reading the code
nearby will not warn you about.

**Table prefixes are enforced, not stylistic.** Every app gets its own SQLite
pool with an authorizer that denies reads *and* writes to any table not
prefixed with its own app key (`db::init_scoped`). A table named without the
`<app_key>_` prefix is invisible to the app that owns it, and the failure
surfaces as an authorization error from SQLite, not as a missing table.

**A `sqlx::FromRow` struct and its `SELECT`s drift silently.** Queries are
runtime-checked, so adding or removing a field compiles fine — every `SELECT`
that builds that struct has to be updated by hand. A mismatch makes the row fail
to map, and the usual `.unwrap_or_else(|e| { tracing::error!(…);
Default::default() })` turns that into an *empty result*, so the page renders
"no data" rather than an error and the log line is easy to miss. When you change
one of these structs, grep for every query that names it.

The one automated hint you get is the dead-code warning on a field nothing
reads, so put `#[allow(dead_code)]` on the *field* that is deliberately unused,
never on the struct — a struct-level allow silences every field at once and
takes the signal with it.

**CSS has no scoping.** Every app's stylesheet is concatenated into one
`/static/apps.css` served on every page, so a bare `table { … }` in one app
restyles every other app. Scope every rule to an app-specific class. For tables
that collapse into cards on phones, opt into the shared `table-cards` utility
in `static/core.css` and add only cell placement locally.

Scoping to a class is not enough on its own, because `static/core.css` styles
bare elements and an element selector there outranks a class here:
`button[type="submit"]` paints any icon button with the accent fill, and
`form { display: flex }` silently beats the UA's `[hidden] { display: none }`, so
a `hidden` form stays open. Check what core.css already says about an element
before styling it, and use `:has()` or a second class when you need to outrank
it. App class names are not reserved either: two apps can define the same
class and the later one in the concatenation order (`all_app_instances`) wins
silently, in whichever direction hurts. There are no collisions left today, and
this should stay empty (per file first, or it just finds each rule twice):

```sh
for f in static/core.css crates/*/static/style.css; do
  grep -o '^\.[a-zA-Z][a-zA-Z0-9_-]*' "$f" | sort -u
done | sort | uniq -d
```

Prefix a new shared-sounding class with the app key.

**A horizontal swipe anywhere empty changes tab.** `static/nav-swipe.js` is
inlined into every page by `layout.rs`, and on a phone it drags `<main>` with
the finger and navigates to the neighbouring nav item. It bows out when the
gesture starts on a control, a chart or anything that scrolls sideways — but
that list is a CSS selector (`SKIP`) plus an overflow check, not an inference.
A new widget that handles its own horizontal drag has to be named there, or
`[data-no-swipe]` put on it; otherwise the swipe claims the gesture with
`preventDefault()` and the widget never sees it. A widget that handles
*vertical* drags needs `touch-action: none` for the same reason the window
selector has it.

The script is in `<head>`, and it has to stay there: it arms the slide-in of
the page just swiped to (`data-nav-swipe-in` on `<html>`, styled in
`core.css`), which must be set before `<main>` first paints. Moved to the end
of `<body>` it still *works* — the page appears at rest for a frame, then
jumps offscreen to slide back in.

**Nothing in CI parses `static/*.js`.** The files are `include_str!`'d into the
page as strings, so a misspelt identifier is not a build error but a
`ReferenceError` thrown mid-gesture in one browser — `make check` passes and
the feature is simply dead on a phone. Check a JS change actually runs; a
`node --check` only catches syntax, not an undeclared name.

Page scripts live in `static/*.js` (core) or a crate's `static/`, and take
**no interpolation**: they read the base path from `<html data-base>`, short
strings from data attributes on the element they drive, and anything larger
from a `<script type="application/json">` tag escaped with
`components::json_for_script`. Putting a `{…}` back into one means brace-escaping
the whole file again, so pass the value through the DOM instead. Because the
attribute names are the contract and nothing type-checks them, each extracted
script has a test asserting its attributes are still rendered — add one when
you add a script, or a rename will fail silently in the browser only.

**Handlers build HTML with `format!`, which escapes nothing.** Any user- or
provider-supplied string (account names, labels, transaction descriptions,
counterparties, filenames) must pass through
`myapps_core::components::html_escape` first. It escapes both quote characters,
so it is safe in element bodies and quoted attribute values. `<option>` bodies
are *not* a safe sink — the browser re-parses entity-decoded text there and
builds live elements.

A `<script>` body is a different sink again, and `html_escape` is the wrong
tool for it: use `components::json_for_script`, which escapes the markup
characters so a value containing `</script>` cannot close the element, or
`components::call_script` where you are emitting a one-call fragment. Keep
these in `components.rs`; the codebase has carried three divergent copies of
this escaping before.

**FileClipboard is the only app with state outside SQLite.** Contents live at
`FILE_CLIPBOARD_DIR/<user_id>/<uuid>`, metadata in `file_clipboard_files`.
Uploads stream to disk in chunks (never buffer a whole one in memory); downloads
are always `attachment` + `nosniff`, because user bytes served inline on the
session origin are stored XSS. Deleting a row does not delete the file —
`services::retention` reconciles disk against the table. Its directory also
needs a systemd `ReadWritePaths` entry when it sits outside the deploy dir —
and a deploy will not add it, because the unit is owned by the `myapps` Ansible
role in the sibling `infra` repo, not by `deploy.sh`; see
[deployment docs](docs/deployment.md#fileclipboard-storage).

**Translations are compile-time structs.** Adding a field to a translation
struct forces both EN and ES to be filled in — that is the point, don't work
around it. Shared strings live in `crates/myapps-core/src/i18n/`, app strings in
each crate's `i18n.rs`.

**Actions belong in the app's `ops.rs`.** Both HTTP handlers and the command bar
dispatcher call into it. An action implemented only in a handler is invisible to
the command bar.

**Migrations from all crates are merged by timestamp** (`db::migrator()`) and
run on startup, including in production, with no backup step. Core migrations
live in `crates/myapps-core/migrations/`, app ones in each crate's
`migrations/`. Timestamps are the primary key, so they must not collide across
crates. The migrator runs with `ignore_missing: true` — a migration deleted or
retimestamped after it shipped will not fail a deployed database, and will also
not be re-applied to it. So a migration that has reached any environment is
frozen: correct it with a *new* migration, never by editing the old one.

**Memory is the binding constraint** — 4 GB shared with whisper.cpp and
llama.cpp. Prefer borrowing over cloning, and weigh any new dependency.

**Adding or removing an environment variable means five files**, and missing one
fails silently at runtime rather than at build time:
`.env.example`, `deploy/*.env.example`, the `.env` template in `deploy.sh`
(`setup()`), the generated deploy config in `.github/workflows/cd.yml`
(for `DEPLOY_*` variables), and the Environment Variables table in
`docs/deployment.md`.

**Bump the version in `Cargo.toml` before merging to `main`** — CD fails the
release job if the version is not higher than the latest tag.
`make bump-{patch,minor,major}`; `/finish-development` does it for you.

## Workflows

- `./devbox.sh create <branch>` — develop in a microVM with no credentials in
  it: a clone, the toolchain and Claude Code inside, every secret brokered from
  the host. `sandbox/README.md`; `./devbox.sh doctor` first.
  `./devbox.sh grant <branch> prod-readonly` then gives the sandbox
  `devbox-prod snapshot|logs|status` — the real database, scrubbed of every
  credential, and prod's journal, through a host broker that holds the SSH key
  and answers three read verbs.
- `/add-app <AppName>` — scaffold a new app crate and wire it into the workspace.
- `/finish-development` — version bump, docs, PR.
- **frontend-tester** agent (`.claude/agents/frontend-tester.md`) — write or
  update `axum-test` integration tests after a frontend change. App tests live
  in each crate's `tests/`; platform auth/launcher tests in the root `tests/`;
  helpers in `crates/myapps-test-harness/`.
- `/frontend-walkthrough` — drive a real browser over the routes the branch
  touched, for what tests can't see (XSS, broken swaps, console errors, layout).

## Docs

[Requirements](docs/requirements.md) ·
[Architecture](docs/architecture.md) ·
[Deployment](docs/deployment.md) ·
[Worktrees](docs/worktree-workflow.md) ·
[Sandboxed development](sandbox/README.md)
