Scaffold a new app in the MyApps workspace.

Usage: `/add-app <AppName>` — PascalCase, e.g. `BudgetPlanner`.

Everything derives from that name: crate `myapps-budget-planner`, module
`myapps_budget_planner`, app key and table prefix `budget_planner`, route
prefix `/budget_planner`.

Ask for a one-line description and an icon (one emoji) before starting.

## 1. Copy the shape of an existing app

`crates/myapps-form-input/` is the reference — it is the smallest app that uses
every part of the `App` trait. Read it and mirror its layout: `Cargo.toml`,
`src/lib.rs` (router + the `App` impl), `i18n.rs`, `ops.rs`,
`services/seed.rs`, `static/style.css`, `migrations/`, `README.md`, and one
feature module so the app has a page worth visiting.

Take the trait method signatures from that crate rather than from memory — the
`App` trait in `myapps_core::registry` is the authority on what is required and
what has a default.

Things that are easy to get wrong here:

- **Table names must start with `<app_key>_`.** This is enforced at runtime by
  the per-app SQLite authorizer, not just a convention — an unprefixed table is
  unreadable by its own app. Every user-scoped table needs
  `user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE` and an
  index on it.
- **The migration filename is a timestamp** (`YYYYMMDD000000_<app_key>.sql`) and
  it is the primary key across every crate's migrations. Check no existing
  migration already uses it.
- **CSS needs an app-specific class prefix.** All app stylesheets are
  concatenated into one file, so a bare element selector restyles every other
  app. Build on the design tokens in `static/core.css`.
- **`i18n.rs` must define every field in both EN and ES** — the compiler will
  tell you if it doesn't. Start with a handful of strings.
- **`seed()` calls `myapps_core::registry::delete_user_app_data` first**, so
  re-seeding is idempotent.

## 2. Wire it in

Four edits, none of them discoverable from the new crate:

- Root `Cargo.toml`: add the path to `[workspace.dependencies]` and the crate
  to `[dependencies]`.
- `src/lib.rs`: re-export it in the `apps` module and add
  `Box::new(myapps_<snake>::<AppName>App)` to `all_app_instances()`.
- `.env.example` and `docs/deployment.md`: add the key to the two
  `DEPLOY_APPS` "valid keys" lists.
- `README.md` and `docs/architecture.md`: add a row to the apps table and a
  line to the crate tree.

CLAUDE.md needs nothing unless the app introduces a gotcha worth warning the
next person about — it documents traps, not an inventory of apps.

Run `cargo check` here.

## 3. Test it

Add `crates/myapps-<kebab>/tests/integration.rs` declaring one module per
feature area, and write the first two tests: the route redirects when logged
out, and it renders when logged in.

```rust
async fn app() -> myapps_test_harness::TestApp {
    myapps_test_harness::spawn_app(vec![Box::new(<AppName>App)]).await
}
```

See `.claude/agents/frontend-tester.md` for the harness API and its gotchas —
notably that a 303 needs `.expect_failure()`.

## 4. Finish

`make check`, then screenshots: add a section to `scripts/screenshots.ts`
following the existing pattern, run `make screenshots`, and add the `<img>`
tags to the root `README.md` and the app's own `README.md`.
