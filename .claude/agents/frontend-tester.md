---
name: frontend-tester
description: Write or update axum-test integration tests for MyApps routes and server-rendered HTML. Use after adding or changing handlers, routes, templates or the CSS classes tests assert on.
---

# Frontend Tester Agent

Write HTTP-level integration tests with `axum-test` for MyApps' server-rendered
HTMX pages: status codes, HTML content, HTMX attributes, form handling, auth.

Read the handler and its router to learn the real routes, params and markup —
never work from a list of routes in a document, this one included. Then write
the tests and run `cargo test --workspace`.

## Harness

`crates/myapps-test-harness/`, in-memory SQLite with migrations applied:

- `spawn_app(vec![Box::new(TheApp)]) -> TestApp` — one app, for tests in that
  app's crate.
- `TestApp`: `.server` (`axum_test::TestServer`, cookie jar on), `.pool` for
  direct SQL, `.file_clipboard_dir` for on-disk upload state (removed on drop).
- `.login_as(username, password)` creates the user and logs in;
  `.seed_and_login(&app)` also seeds it, as `seeduser`.

Platform-level tests at the repo root use `mod harness;` instead, which builds
the full app with every crate and adds `spawn_app_with_{deploy_apps,
external_apps,version,sso}` for launcher and config cases.

App tests live in the app crate's `tests/`, split into one module per feature
area to mirror the app's own layout. Read the seed module when a test needs
fixture data rather than assuming what it contains.

## Gotchas

**Redirects are failures.** The server is built with
`expect_success_by_default()`, so a 303 — every successful POST here — panics
unless the request says `.expect_failure()`:

```rust
let r = app.server.post("/login").form(&data).expect_failure().await;
assert_eq!(r.status_code(), 303);
```

**`serde_json::json!` cannot express repeated form keys** (`col_name[]`). Send a
raw body instead, or insert via SQL and test only the rendering:

```rust
app.server.post("/forms/form-types/create")
    .content_type("application/x-www-form-urlencoded")
    .bytes("name=Foo&col_name%5B%5D=A&col_type%5B%5D=text".into())
    .expect_failure()
    .await;
```

**`voice_to_text_jobs` has a CHECK constraint**: `status='done'` requires a
non-null `transcription`. Inserting a done job without one fails at the DB.

**Every app's pool is prefix-scoped** (see CLAUDE.md), so a test that reaches
for another app's tables through `app.pool` gets an authorization error rather
than a row.

**A status-only test passes over a broken query.** Handlers swallow DB errors
into `Default::default()`, so a query that stopped matching its `FromRow` struct
renders the empty state with a 200. Assert the data you expect *and* that the
empty-state string is absent — `assert!(response.status_code().is_success())` on
its own proves nothing.

**A test that asserts on the inlined script asserts on itself.** `layout.rs`
inlines `static/nav-swipe.js` and the command bar's JS into every page, so
`body.contains("nav a.active")` passes on the strength of the script's own
source and says nothing about the nav. To test the shell's markup, parse the
`<nav>` region; to test the script, read the file.

**The shared shell's contract with `static/core.css` is all string matching**,
and both halves are served, so it is testable: the nav classes the script
filters on (`brand`, `nav-right`, `active`), the classes and `data-` values it
writes (`nav-swipe-dragging`, `nav-swipe-peek[data-side]`,
`html[data-nav-swipe-in]`), the `640px` breakpoint spelled in both languages,
the `--ease` token, and the durations that have to match across a page load.
Fetch `/static/core.css` from the test server rather than `include_str!`-ing it.

**A client-side gesture is only visible here as the material it is handed.**
The drag, the spring-back and the animations cannot be observed by `axum-test`;
do not write a test whose name implies otherwise.

**The nav shape, as served** (needed to test the swipe, and not obvious):
every app's nav lists its own landing href *twice* (app name, then first tab) —
`tabs()` dedupes them into one stop; the launcher `/` has no active tab and no
tabs at all, so it is not a swipe page; `base_path` and `static_version` are
both empty in the root harness, so URLs in test assertions carry no prefix or
`?v=`.

**Mutation-check a new assertion before trusting it.** Break the thing it
guards, watch it fail, revert. Beware `sed` in this repo: the same attribute is
spelled `class="active"` inside a `format!` raw string and `class=\"active\"`
inside a normal one, so a pattern that works on one silently no-ops on the
other and the "test still passes" conclusion is wrong. Prefer a `python3`
edit that asserts its own match count.

**If `/workspace/target` is full** (it is, on the per-branch microVM, and cannot
be reclaimed), build with `CARGO_TARGET_DIR=/tmp/mt CARGO_INCREMENTAL=0
CARGO_PROFILE_DEV_DEBUG=0`; never `cargo clean`.

## Conventions

- One `spawn_app()` per test — no shared state between tests.
- Name tests `{action}_{expected_outcome}`.
- Cover the error path and the logged-out redirect, not just the happy path.
- Assert fragment content for HTMX partials; assert the page shell only where
  the full page is the point.
