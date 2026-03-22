Scaffold a new app in the MyApps workspace.

Usage: `/add-app <AppName>` — where AppName is PascalCase (e.g. `BudgetPlanner`).

Derive from the name:
- **crate name**: `myapps-<kebab-case>` (e.g. `myapps-budget-planner`)
- **module name**: `myapps_<snake_case>` (e.g. `myapps_budget_planner`)
- **app key**: `<snake_case>` (e.g. `budget_planner`) — used as DB table prefix
- **route prefix**: `/<snake_case>` (e.g. `/budget_planner`)

Ask the user for a one-line description and an icon (single emoji or character)
before proceeding. Then follow every step below. Run `cargo check` after step 4,
and `make check` at the end.

---

## 1. Create the crate

Create `crates/myapps-<kebab>/` with this structure:

### `Cargo.toml`
```toml
[package]
name = "myapps-<kebab>"
version = "0.1.0"
edition.workspace = true

[dependencies]
myapps-core.workspace = true
axum.workspace = true
sqlx.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
tracing.workspace = true

[dev-dependencies]
myapps-test-harness.workspace = true
tokio.workspace = true
sqlx.workspace = true
serde_json.workspace = true
```

### `src/lib.rs`
- Declare modules: `pub mod i18n;`, `pub mod ops;`, `pub mod services;`, plus
  one module per feature page.
- Define a `router()` function returning `Router<AppState>`.
- Define a `pub struct <AppName>App;` implementing the `App` trait:
  - `info()` — key, name, description, icon, path
  - `description(lang)` — EN and ES variants
  - `css()` — `include_str!("../static/style.css")`
  - `migrations()` — `sqlx::migrate!("./migrations")`
  - `router()` — delegate to the module-level `router()`
  - `commands()` — delegate to `ops::commands()`
  - `dispatch()` — delegate to `ops::dispatch()`
  - `seed()` — delegate to `services::seed::run()` (wrap in `Some(Box::pin(...))`)

Use `crates/myapps-classroom-input/src/lib.rs` as a reference for the exact
trait method signatures.

### `src/i18n.rs`
Follow the pattern in `crates/myapps-classroom-input/src/i18n.rs`:
- `pub struct Translations` with `&'static str` fields for every user-facing string.
- `const EN: Translations` and `const ES: Translations` — both must define
  every field (compile-time enforced).
- `pub fn t(lang: myapps_core::i18n::Lang) -> &'static Translations`.

Start with a minimal set of strings (title, subtitle, nav label) and expand as
features are added.

### `src/ops.rs`
Follow the pattern in `crates/myapps-classroom-input/src/ops.rs`:
- Reusable action functions (called from both handlers and the command bar).
- `pub fn commands() -> Vec<CommandAction>` — start empty (`vec![]`).
- `pub async fn dispatch(...)` — start with a catch-all error.

### `src/services/mod.rs` and `src/services/seed.rs`
- `mod.rs`: `pub mod seed;`
- `seed.rs`: `pub async fn run(pool, user_id, app) -> anyhow::Result<()>`
  that calls `myapps_core::registry::delete_user_app_data(pool, app, user_id)`
  first, then inserts demo data.

### `static/style.css`
Create an empty file (or minimal styles). Use the app key as a CSS class
prefix to avoid collisions (e.g. `.bp-card`). Use the shared design tokens
(`--bg`, `--border`, `--accent`, `--text`, etc.) from `static/core.css`.

### `migrations/<timestamp>_<app_key>.sql`
- Use today's date as timestamp: `YYYYMMDD000000`.
- Prefix all table names with `<app_key>_`.
- Include `user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE` on
  every user-scoped table.
- Add indexes on `user_id` columns.

### Initial feature module (e.g. `src/dashboard.rs`)
Create at least one route handler so the app is visitable. A simple dashboard
page that renders an empty state is enough to start.

### `README.md`
```markdown
# <AppName>

<One-line description>.

## Features

- (list key features as they are built)
```

---

## 2. Wire into the workspace

### `Cargo.toml` (root)
Add to `[workspace.dependencies]`:
```toml
myapps-<kebab> = { path = "crates/myapps-<kebab>" }
```
Add to `[dependencies]`:
```toml
myapps-<kebab>.workspace = true
```

### `src/lib.rs`
Add re-export in the `apps` module:
```rust
pub use myapps_<snake> as <snake>;
```
Add to `all_app_instances()`:
```rust
Box::new(myapps_<snake>::<AppName>App),
```

---

## 3. Update documentation

### `CLAUDE.md`
- Add a bullet under "Project Conventions":
  `- <AppName>-specific routes and handlers live in `crates/myapps-<kebab>/`.`
- Update the table-prefix list to include the new app's prefix.

### `README.md` (root)
Add a row to the Apps table:
```markdown
| **<AppName>** (<icon>) | [`myapps-<kebab>`](crates/myapps-<kebab>/) | <description> |
```

### `.env.example`
Update the `DEPLOY_APPS` comment to list the new app key.

---

## 4. Verify

Run `cargo check` to confirm the crate compiles and integrates correctly.

---

## 5. Add integration tests

Create `crates/myapps-<kebab>/tests/integration.rs`:
```rust
mod <snake> {
    pub mod dashboard;  // (or whatever modules you have)
}
```

Create test files under `crates/myapps-<kebab>/tests/<snake>/`. Each test file
should follow this pattern:
```rust
use myapps_<snake>::<AppName>App;

async fn app() -> myapps_test_harness::TestApp {
    myapps_test_harness::spawn_app(vec![Box::new(<AppName>App)]).await
}

#[tokio::test]
async fn dashboard_requires_authentication() {
    let app = app().await;
    let r = app.server.get("/<snake>").expect_failure().await;
    assert_eq!(r.status_code(), 303);
}

#[tokio::test]
async fn dashboard_renders() {
    let app = app().await;
    app.login_as("test", "pass").await;
    let r = app.server.get("/<snake>").await;
    assert!(r.text().contains("<AppName>"));
}
```

---

## 6. Final check

Run `make check` (fmt + clippy + test). Fix any issues. Done.
