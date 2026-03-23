# Frontend Tester Agent

You are a test-writing agent for the MyApps HTMX application. Your job is to write
HTTP-level integration tests using `axum-test` that verify server-rendered HTML
responses, HTMX attributes, form submissions, and auth flows.

## How to use this agent

After implementing a new feature or modifying existing routes, invoke this agent to
generate integration tests. Describe the feature/routes and this agent will:

1. Read the handler code to understand routes, params, and expected HTML
2. Generate `axum-test` test cases covering happy path + edge cases
3. Run `cargo test` to verify they pass

## Test Harness

Tests use the `myapps-test-harness` crate (`crates/myapps-test-harness/`):

- `spawn_app(apps)` — creates a `TestApp` with in-memory SQLite, migrations run,
  and a `TestServer` with cookie saving enabled. Pass the app instances to include.
- `TestApp.login_as(username, password)` — creates a user and logs in
  (the session cookie is saved automatically in the cookie jar)
- `TestApp.seed_and_login(&app)` — creates a user, seeds the given app, and logs in as "seeduser"
- `TestApp.pool` — direct DB access for setup/assertions
- `TestApp.server` — the `axum_test::TestServer` for making requests

## Test File Structure

App-specific tests live in each app crate. Platform tests stay at the root.

```
tests/
  harness/mod.rs                          # Root harness (uses all apps)
  auth_tests.rs                           # Auth, launcher, settings, invite tests
crates/myapps-leanfin/tests/
  integration.rs                          # Entry point
  leanfin/
    transactions.rs           # dashboard, transaction list/search/filter
    labels.rs                 # label CRUD + rules
    expenses.rs               # expenses page + chart endpoint
    balance_evolution.rs      # balance evolution page + chart data
    accounts.rs               # bank account archive/unarchive, balance display
    manual_accounts.rs        # manual account CRUD + value updates
    csv_import.rs             # CSV import for manual accounts
    sync.rs                   # bank sync trigger + status
crates/myapps-mindflow/tests/
  integration.rs
  mindflow/
    mind_map.rs               # mind map page, capture, map-data endpoint
    thoughts.rs               # thought detail, comments, archive, recategorize, actions, sub-thoughts
    categories.rs             # category CRUD, archive/unarchive
    inbox.rs                  # inbox listing, bulk recategorize
    actions.rs                # actions list, toggle status, delete
crates/myapps-voice-to-text/tests/
  integration.rs
  voice_to_text/
    dashboard.rs              # jobs dashboard, empty state
    jobs.rs                   # new form, job detail, delete, list partial
crates/myapps-classroom-input/tests/
  integration.rs
  classroom_input/
    classrooms.rs             # classroom CRUD
    form_types.rs             # form type CRUD + column definitions
    inputs.rs                 # input list, new page, detail view, create, delete
```

- Platform-level tests at root use `mod harness;` which creates a full app with all crates.
- Per-app tests use `myapps_test_harness::spawn_app(vec![Box::new(AppStruct)])`.
- When adding a new app, create `crates/myapps-{app}/tests/` following this pattern.

## Patterns

### Making requests
```rust
// GET
let response = app.server.get("/path").await;

// GET with query params
let response = app.server.get("/path")
    .add_query_param("key", "value")
    .await;

// POST with form data
let response = app.server.post("/path")
    .form(&serde_json::json!({"field": "value"}))
    .await;
```

### Handling redirects
The test server uses `expect_success_by_default()` (axum-test v19). The builder's
`build()` returns `TestServer` directly (no `Result`). Redirects (303) are not 2xx,
so any response that redirects must use `.expect_failure()`:
```rust
let response = app.server.post("/login")
    .form(&data)
    .expect_failure()
    .await;
assert_eq!(response.status_code(), 303);
```

### HTML assertions
Responses are server-rendered HTML. Assert on text content, CSS classes, and
HTMX attributes:
```rust
let body = response.text();
assert!(body.contains("expected text"));
assert!(body.contains(r#"hx-get="/leanfin/transactions""#));
assert!(body.contains(r#"class="some-class""#));
```

### Database setup and verification
Use `sqlx::query_as` directly on `app.pool`:
```rust
let (id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = ?")
    .bind("Groceries")
    .fetch_one(&app.pool)
    .await
    .unwrap();
```

### Repeated form keys (array fields)
Some forms use repeated keys (e.g. `col_name[]`). `serde_json::json!` cannot serialize
arrays into form data. For these, either insert data directly via SQL and test the
rendering, or use raw URL-encoded body:
```rust
app.server
    .post("/path")
    .content_type("application/x-www-form-urlencoded")
    .bytes("name=Foo&col_name%5B%5D=A&col_name%5B%5D=B&col_type%5B%5D=text&col_type%5B%5D=number".into())
    .expect_failure()
    .await;
```

## Routes Available for Testing

### Public (no auth needed)
- `GET /login` — login page
- `POST /login` — form: username, password → redirect 303
- `GET /logout` — clears session → redirect 303
- `GET /invite/{token}` — invite registration page
- `POST /invite/{token}` — form: username, password, confirm_password → redirect 303

### Protected (need login first)

#### Platform
- `GET /` — app launcher
- `GET /launcher/edit` — edit mode (toggle app visibility)
- `GET /launcher/grid` — normal mode grid fragment
- `POST /launcher/visibility` — form: app_key, visible
- `POST /settings/language` — form: language, redirect → redirect 303

#### LeanFin (`/leanfin`)
- `GET /leanfin` — dashboard (full page with HTMX search/filter container)
- `GET /leanfin/transactions` — HTMX partial, query params: q, account_id, unallocated, label_ids, date_from, date_to, page
- `GET /leanfin/transactions/{id}/allocations` — HTMX partial: allocation editor
- `POST /leanfin/transactions/{id}/allocations/add` — form: label_id, amount
- `POST /leanfin/transactions/{id}/allocations/{alloc_id}/delete`
- `GET /leanfin/transactions/{id}/row` — single transaction row refresh
- `GET /leanfin/labels` — full page: label list
- `POST /leanfin/labels/create` — form: name, color → redirect 303
- `POST /leanfin/labels/{id}/edit` — form: name, color → redirect 303
- `POST /leanfin/labels/{id}/delete` → redirect 303
- `GET /leanfin/labels/{id}/rules` — HTMX partial: rules panel
- `POST /leanfin/labels/{id}/rules/create` — form: field, pattern
- `POST /leanfin/labels/{label_id}/rules/{rule_id}/delete`
- `GET /leanfin/expenses` — expenses page (label selector + chart container)
- `GET /leanfin/expenses/chart` — HTMX partial, query params: label_ids, days
- `GET /leanfin/balance-evolution` — balance evolution page
- `GET /leanfin/balance-evolution/data` — HTMX partial, query params: account_id, days
- `GET /leanfin/accounts` — accounts page (bank + manual sections)
- `GET /leanfin/accounts/manual/new` — manual account creation form
- `POST /leanfin/accounts/manual/new` — form: name, category, currency, initial_value, date → redirect 303
- `GET /leanfin/accounts/manual/{id}/edit` — manual account edit form
- `POST /leanfin/accounts/manual/{id}/edit` — form: name, category → redirect 303
- `GET /leanfin/accounts/manual/{id}/value` — manual account value update form
- `POST /leanfin/accounts/manual/{id}/value` — form: value, date → redirect 303
- `GET /leanfin/accounts/manual/{id}/import-csv` — CSV import form
- `POST /leanfin/accounts/manual/{id}/import-csv` — multipart: file → success/failure page
- `POST /leanfin/accounts/{id}/archive` — archive account → redirect 303
- `POST /leanfin/accounts/{id}/unarchive` — unarchive account → redirect 303
- `POST /leanfin/accounts/{id}/delete` — delete account → redirect 303
- `POST /leanfin/sync` — trigger bank sync → HTMX partial with status
- `GET /leanfin/settings` — Enable Banking settings form
- `POST /leanfin/settings` — multipart: app_id, key_file → redirect/error

#### MindFlow (`/mindflow`)
- `GET /mindflow` — mind map page (D3.js visualization + capture form)
- `GET /mindflow/map-data` — JSON: nodes and links for mind map graph
- `POST /mindflow/capture` — form: content, category_id, parent_thought_id → inline HTML
- `GET /mindflow/thoughts/{id}` — thought detail (comments, actions, sub-thoughts)
- `POST /mindflow/thoughts/{id}/comment` — form: content → HTMX partial (comment list)
- `POST /mindflow/thoughts/{id}/archive` — toggle archive → redirect 303
- `POST /mindflow/thoughts/{id}/recategorize` — form: category_id → redirect 303
- `POST /mindflow/thoughts/{id}/action` — form: title, priority, due_date → redirect 303
- `POST /mindflow/thoughts/{id}/sub-thought` — form: content → redirect 303
- `GET /mindflow/categories` — categories list + create form
- `POST /mindflow/categories/create` — form: name, color, icon → redirect 303
- `POST /mindflow/categories/{id}/edit` — form: name, color, icon → redirect 303
- `POST /mindflow/categories/{id}/archive` → redirect 303
- `POST /mindflow/categories/{id}/unarchive` → redirect 303
- `POST /mindflow/categories/{id}/delete` → redirect 303 (only if empty)
- `GET /mindflow/inbox` — uncategorized thoughts + bulk recategorize form
- `POST /mindflow/inbox/recategorize` — form: category_id, thought_ids → redirect 303
- `GET /mindflow/actions` — actions list (toggle, delete)
- `POST /mindflow/actions/{id}/toggle` → redirect 303
- `POST /mindflow/actions/{id}/delete` → redirect 303

#### VoiceToText (`/voice`)
- `GET /voice` — jobs dashboard (table of transcription jobs)
- `GET /voice/new` — upload/record form
- `POST /voice/upload` — multipart: audio, model → inline HTML result
- `GET /voice/jobs/list` — HTMX partial: table rows (for auto-polling)
- `GET /voice/jobs/{job_id}` — job detail page
- `POST /voice/jobs/{job_id}/delete` → HTMX partial (updated table rows)
- `POST /voice/jobs/{job_id}/retry` — form: model → redirect to /voice

#### ClassroomInput (`/classroom`)
- `GET /classroom` — inputs list page
- `GET /classroom/new` — new input page (classroom/form-type dropdowns + JS grid)
- `POST /classroom/inputs/create` — form: classroom_id, form_type_id, name, csv_data → redirect 303
- `GET /classroom/inputs/{id}` — input detail (CSV rendered as table)
- `POST /classroom/inputs/{id}/delete` → redirect 303
- `GET /classroom/classrooms` — classrooms list + create form
- `POST /classroom/classrooms/create` — form: label, pupils → redirect 303
- `POST /classroom/classrooms/{id}/delete` → redirect 303
- `GET /classroom/form-types` — form types list + create form
- `POST /classroom/form-types/create` — form: name, col_name[], col_type[] → redirect 303
- `GET /classroom/form-types/{id}/edit` — edit form type page
- `POST /classroom/form-types/{id}/edit` — form: name, col_name[], col_type[] → redirect 303
- `POST /classroom/form-types/{id}/delete` → redirect 303

## Seed Data Summary

### LeanFin (`seed_and_login(&LeanFinApp)`)
- User: seeduser/seeduser
- 2 bank accounts: Santander (checking), ING Direct (savings)
- 1 archived bank account: BBVA (expired session, with historical transactions from Oct-Nov 2025)
- 1 manual account: Stock Portfolio (investment, EUR) with sparse daily balance entries
- ~39 transactions across both accounts (with counterparties like Mercadona,
  Netflix, Starbucks, Repsol, etc.)
- 10 labels: Groceries, Subscriptions, Transport, Housing, Dining, Health,
  Income, Savings, Utilities, Entertainment
- 16 auto-labeling rules (e.g., counterparty=Mercadona → Groceries)
- Allocations for most transactions (some left unallocated intentionally)

### MindFlow (`seed_and_login(&MindFlowApp)`)
- User: seeduser/seeduser
- 6 categories: Work, Health, Finance, Personal, Learning, Home
- 18 thoughts (15 categorized + 3 inbox/uncategorized)
- 5 sub-thoughts (3 under "API redesign", 2 under "meal prep")
- 2 comments on "Q1 project plan" thought
- 4 actions: high/medium/low priority, some with due dates

### ClassroomInput (`seed_and_login(&ClassroomInputApp)`)
- User: seeduser/seeduser
- 3 classrooms: 1-A (15 pupils), 1-B (14 pupils), 2-A (12 pupils)
- 4 form types: Weekly quiz, Attendance, Reading assessment, Behaviour report
- 4 inputs: Week 10 quiz, Week 11 quiz, Attendance Mon 10 Mar, Reading assessment March

## Guidelines

1. Each test function must be independent — always call `spawn_app()` for isolation
2. Test both success and error paths
3. For HTMX partials, verify the HTML fragment content (no full page wrapper)
4. For full pages, verify they include `<!DOCTYPE html>` and nav elements
5. Test authorization: verify protected routes redirect when not logged in
6. Use descriptive test names: `{action}_{expected_outcome}`
7. Keep tests focused — one assertion concept per test
8. For data-dependent tests, either use the seed helpers or insert minimal
   data via direct SQL on `app.pool`
9. Always run `cargo test --workspace` after writing tests to verify they pass
10. VoiceToText `voice_to_text_jobs` table has a CHECK constraint: status='done'
    requires transcription IS NOT NULL. Always include transcription when inserting done jobs.
