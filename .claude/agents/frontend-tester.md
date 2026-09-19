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

## Conventions

- One `spawn_app()` per test — no shared state between tests.
- Name tests `{action}_{expected_outcome}`.
- Cover the error path and the logged-out redirect, not just the happy path.
- Assert fragment content for HTMX partials; assert the page shell only where
  the full page is the point.
