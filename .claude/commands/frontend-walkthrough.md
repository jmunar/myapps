Drive a real Chromium browser through the MyApps UI to find frontend bugs the
test suite can't see (XSS, broken HTMX swaps, console errors, 4xx/5xx requests,
broken layouts, dead links).

Usage:

- `/frontend-walkthrough` — **branch mode**. Walk through every page touched
  by the current branch (vs `main`). Use this before opening a PR.
- `/frontend-walkthrough <target>` — **on-demand mode**. Walk a specific area.
  `<target>` can be an app key (`form_input`, `leanfin`, `mindflow`, `voice`,
  `notes`), a route path (`/forms/row-sets`, `/leanfin/labels`), or a free-form
  description ("the new-input grid", "the login flow"). Use this whenever you
  want to spot-check one feature.

The walkthrough always:
1. Runs against an isolated server on port 3198 with a fresh DB and seeded
   `demo` user — production data is never touched.
2. Captures screenshots, console errors, page errors, request failures, and
   every 4xx/5xx response per step.
3. Probes adversarial inputs (XSS payload, length stress, malformed IDs) on
   any reachable form.
4. Cleans up: kills the server, removes the temp DB, and removes the spec/config
   files copied into the repo root.

---

## 1. Parse the target

If the user provided no argument, this is **branch mode**. Skip to step 2.

Otherwise it's **on-demand mode** with the user's target. Map it to a list of
URL paths to visit:

- App key (`form_input`, `leanfin`, `mindflow`, `voice`, `notes`):
  - Read `crates/myapps-<app>/src/lib.rs` and the per-feature modules to find
    the routes the app exposes.
  - Build a list of full-page GET routes plus their main forms.
- Route path (starts with `/`): walk just that path and any links it surfaces.
- Free-form description: ask the user to point you at the relevant
  app/route(s), then proceed as above. Don't guess.

Skip to step 3.

## 2. Branch mode — discover what changed

Run `git diff --name-only main...HEAD` and filter to files that affect the UI:
- `crates/myapps-*/src/**/*.rs` — handlers, templates, ops
- `crates/myapps-*/static/**` — CSS, JS, assets
- `static/**` — shared CSS, JS, manifest

For each changed file, derive which routes are reached:
- A handler module exposes routes via `routes()` — read it to enumerate paths.
- A static asset is reached by visiting any page (the page imports it).
- An i18n change affects every page in the app.

If nothing UI-related changed, tell the user "no frontend changes on this
branch" and stop — don't spin up the server.

Build a deduplicated list of full-page GET paths to walk, plus any forms those
pages expose.

## 3. Build the binary

If `target/debug/myapps` doesn't exist or `Cargo.lock` is newer than the
binary, run `cargo build` (debug build — release is too slow for a feedback
loop). If it's already up to date, skip.

## 4. Set up an isolated server

In **one** background shell:

```bash
DB=/tmp/frontend-walkthrough/test.db
mkdir -p /tmp/frontend-walkthrough
rm -f "$DB" "${DB}-wal" "${DB}-shm"

DATABASE_URL="sqlite://${DB}" BIND_ADDR="127.0.0.1:3198" \
ENCRYPTION_KEY="0000000000000000000000000000000000000000000000000000000000000000" \
./target/debug/myapps create-user --username demo --password demo

DATABASE_URL="sqlite://${DB}" BIND_ADDR="127.0.0.1:3198" \
ENCRYPTION_KEY="0000000000000000000000000000000000000000000000000000000000000000" \
./target/debug/myapps seed --user demo

DATABASE_URL="sqlite://${DB}" BIND_ADDR="127.0.0.1:3198" \
ENCRYPTION_KEY="0000000000000000000000000000000000000000000000000000000000000000" \
./target/debug/myapps serve > /tmp/frontend-walkthrough/server.log 2>&1 &
echo $! > /tmp/frontend-walkthrough/server.pid
```

Poll `http://127.0.0.1:3198/login` until it returns 200 (max ~10 seconds), then
proceed. If the server exits, read `server.log` and stop.

## 5. Install Playwright once

```bash
ls node_modules/.bin/playwright >/dev/null 2>&1 || \
  npm install --save-dev @playwright/test 2>&1 | tail -3
ls ~/Library/Caches/ms-playwright/chromium-* >/dev/null 2>&1 || \
  ./node_modules/.bin/playwright install chromium 2>&1 | tail -3
```

The repo's existing `screenshots.ts` script already uses Playwright, so this
is usually a no-op.

## 6. Generate the walkthrough spec

Write a fresh spec each run at `walkthrough.spec.ts` (in the repo root — required
so `@playwright/test` resolves) and a paired `walkthrough.config.ts` that
points at it. Use the template below as a starting point and add one labelled
section per target route. The script must capture issues into a `report.json`
and exit cleanly even when assertions surface bugs.

```typescript
import { test, type Page, type ConsoleMessage, type Request } from "@playwright/test";
import path from "path";

const BASE_URL = "http://127.0.0.1:3198";
const OUT = "/tmp/frontend-walkthrough";

type Issue = { kind: string; where: string; detail: string };
const issues: Issue[] = [];

function track(page: Page, label: string) {
  page.on("console", (msg: ConsoleMessage) => {
    if (msg.type() === "error" || msg.type() === "warning") {
      issues.push({ kind: `console.${msg.type()}`, where: label, detail: msg.text() });
    }
  });
  page.on("pageerror", (err) => {
    issues.push({ kind: "pageerror", where: label, detail: err.message });
  });
  page.on("requestfailed", (req: Request) => {
    issues.push({
      kind: "requestfailed",
      where: label,
      detail: `${req.method()} ${req.url()} :: ${req.failure()?.errorText}`,
    });
  });
  page.on("response", (resp) => {
    const url = resp.url();
    if (url.startsWith("data:") || url.startsWith("blob:")) return;
    if (resp.status() >= 500) {
      issues.push({ kind: "5xx", where: label, detail: `${resp.status()} ${resp.request().method()} ${url}` });
    } else if (resp.status() === 404) {
      issues.push({ kind: "404", where: label, detail: `${resp.request().method()} ${url}` });
    } else if (resp.status() >= 400 && resp.status() !== 401) {
      issues.push({ kind: "4xx", where: label, detail: `${resp.status()} ${resp.request().method()} ${url}` });
    }
  });
}

async function login(page: Page) {
  await page.goto(`${BASE_URL}/login`);
  await page.fill("#username", "demo");
  await page.fill("#password", "demo");
  await page.click('button[type="submit"]');
  await page.waitForURL(`${BASE_URL}/`);
}

async function snap(page: Page, name: string) {
  await page.waitForTimeout(150);
  await page.screenshot({ path: path.join(OUT, `${name}.png`), fullPage: true });
}

test.describe.configure({ mode: "serial" });
test.use({ viewport: { width: 1280, height: 900 } });

test("walkthrough", async ({ page }) => {
  track(page, "main");
  await login(page);
  await snap(page, "00-launcher");

  // ── For each target route, add a section here ────────────────
  // 1. Navigate
  // 2. Snap a screenshot
  // 3. If the page has a create form: submit happy path, XSS payload, length stress
  // 4. If the page has links: click into one and snap
  // ──────────────────────────────────────────────────────────────

  const fs = await import("fs");
  fs.writeFileSync(path.join(OUT, "report.json"), JSON.stringify({ issues, count: issues.length }, null, 2));
  console.log("WALKTHROUGH ISSUES:", issues.length);
  for (const i of issues) console.log(" -", i.kind, "@", i.where, "::", i.detail);
});
```

Companion config:

```typescript
// walkthrough.config.ts
import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: ".",
  testMatch: "walkthrough.spec.ts",
  timeout: 120_000,
});
```

When generating the per-target sections:

- For every form on a target page, submit at least these inputs:
  - **happy path** — typical valid data
  - **XSS** — `<img src=x onerror="window.__xss_<label>=1">` in any text field;
    after submit, check `await page.evaluate(...)` for the marker and record
    an `XSS` issue if the payload fired
  - **length stress** — 5,000-char strings and 1,000-line bodies on multi-line
    fields, to surface missing length validation
- For HTMX endpoints, watch for partial-render breakage: after a swap, snap
  the page and assert the swap target still has expected content (look for
  `hx-target` on the trigger element).
- For pages that take an `{id}` path param, also visit `/<route>/999999` and
  record the response — a 5xx here is always a bug.

## 7. Run the walkthrough

```bash
./node_modules/.bin/playwright test --config=walkthrough.config.ts \
  --reporter=list --workers=1 2>&1 | tail -40
```

If the test errors at the framework level (not assertion failures), read the
output and fix the spec. Don't stop because assertions failed — assertion
failures are the point.

## 8. Report

Read `/tmp/frontend-walkthrough/report.json` and `/tmp/frontend-walkthrough/server.log`
together. Summarise for the user:

- **Bugs found** — XSS firings, 5xx responses, page errors. For each, point at
  the source file and line you suspect and explain why. These are the things
  to fix.
- **Suspect** — 4xx other than 401, console errors, request failures.
  Investigate before dismissing.
- **Expected** — 401s on protected routes hit while logged out, 404s you
  triggered intentionally with bogus IDs.
- **Screenshots** — list which ones to look at first (anything where a bug
  was reproduced).

If a bug was triggered by an XSS payload, also grep the relevant handler/template
for the unescaped interpolation and quote the offending line, so the fix is
mechanical.

## 9. Clean up

```bash
kill "$(cat /tmp/frontend-walkthrough/server.pid)" 2>/dev/null
rm -f walkthrough.spec.ts walkthrough.config.ts
rm -f /tmp/frontend-walkthrough/test.db /tmp/frontend-walkthrough/test.db-wal /tmp/frontend-walkthrough/test.db-shm
```

Leave the screenshots and `report.json` in place — they're useful for the user
to review after the run.

---

## Notes

- **Don't run this on every change.** It takes 30-60s per app. Use it before
  PR review (branch mode) or when chasing a specific frontend symptom
  (on-demand mode). For correctness regressions, the integration tests
  generated by the `frontend-tester` agent are faster.
- **Adversarial inputs are non-destructive** — they hit the isolated DB only.
- **Don't commit the spec/config files** — clean-up always removes them. If
  the run is interrupted, delete them manually before pushing.
