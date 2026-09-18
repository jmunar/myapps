Drive a real Chromium browser through the MyApps UI to find bugs the test suite cannot see.

That means XSS, broken HTMX swaps, console errors, 4xx/5xx responses, dead
links and broken layouts.

- `/frontend-walkthrough` — **branch mode**: every page the branch touches
  (vs `main`). Use before opening a PR.
- `/frontend-walkthrough <target>` — **on-demand mode**: an app key, a route
  path, or a description of a feature.

It runs against an isolated server on port 3198 with a throwaway DB and a
seeded `demo` user, so production data is never involved. Don't run it on every
change — it costs 30–60s per app, and the integration tests are the faster loop
for correctness regressions.

## 1. Decide what to walk

**On-demand**: for an app key, read `crates/myapps-<app>/src/lib.rs` and its
feature modules for the routes it actually serves (the launcher lists the keys).
For a route path, walk that path and what it links to. For a description, ask
which routes they mean rather than guessing.

**Branch mode**: `git diff --name-only main...HEAD`, filtered to
`crates/myapps-*/src/**/*.rs`, `crates/myapps-*/static/**` and `static/**`.
Map each changed handler to its routes; a changed asset or i18n file means
every page in that app. If nothing UI-related changed, say so and stop —
don't start the server.

Either way you end with a deduplicated list of full-page GET paths plus the
forms they expose.

## 2. Start an isolated server

Build first if `target/debug/myapps` is missing or older than `Cargo.lock`
(`cargo build` — debug; release is too slow to iterate on).

```bash
mkdir -p /tmp/frontend-walkthrough
DB=/tmp/frontend-walkthrough/test.db
rm -f "$DB" "$DB"-wal "$DB"-shm
export DATABASE_URL="sqlite://$DB" BIND_ADDR="127.0.0.1:3198" \
       ENCRYPTION_KEY="$(printf '0%.0s' {1..64})"

./target/debug/myapps create-user --username demo --password demo
./target/debug/myapps seed --user demo
./target/debug/myapps serve > /tmp/frontend-walkthrough/server.log 2>&1 &
echo $! > /tmp/frontend-walkthrough/server.pid
```

Poll `/login` until it returns 200 (~10s max). If the process exits, read
`server.log` and stop.

Playwright is already a dev dependency for `scripts/screenshots.ts`, so this is
usually a no-op:

```bash
ls node_modules/.bin/playwright >/dev/null 2>&1 || npm install --save-dev @playwright/test
./node_modules/.bin/playwright install chromium   # no-op when already present
```

## 3. Generate and run the spec

Write `walkthrough.spec.ts` and `walkthrough.config.ts` fresh each run, in the
repo root (required for `@playwright/test` to resolve). The template below
carries the part worth keeping stable — how a response is classified as a bug.
Add one labelled section per target route.

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

  // One section per target route: navigate, snap, exercise its forms,
  // follow one link deeper.

  const fs = await import("fs");
  fs.writeFileSync(path.join(OUT, "report.json"), JSON.stringify({ issues, count: issues.length }, null, 2));
  console.log("WALKTHROUGH ISSUES:", issues.length);
  for (const i of issues) console.log(" -", i.kind, "@", i.where, "::", i.detail);
});
```

```typescript
// walkthrough.config.ts
import { defineConfig } from "@playwright/test";
export default defineConfig({ testDir: ".", testMatch: "walkthrough.spec.ts", timeout: 120_000 });
```

For each target, hit every form three ways — valid data, an XSS payload
(`<img src=x onerror="window.__xss_<label>=1">`, then `page.evaluate` for the
marker), and length stress (5,000-char strings, 1,000-line bodies). After an
HTMX swap, snap and confirm the `hx-target` still holds sensible content. For
any route taking an `{id}`, also request `/<route>/999999`: a 5xx there is
always a bug.

```bash
./node_modules/.bin/playwright test --config=walkthrough.config.ts --reporter=list --workers=1 2>&1 | tail -40
```

Assertion failures are the point — keep going. Only framework-level errors mean
the spec itself needs fixing.

## 4. Report, then clean up

Read `report.json` alongside `server.log` and sort what you found:

- **Bugs** — XSS that fired, 5xx, page errors. Name the file and line you
  suspect and why. For an XSS hit, grep the handler for the unescaped
  interpolation and quote the line so the fix is mechanical.
- **Suspect** — non-401 4xx, console errors, failed requests. Investigate
  before dismissing.
- **Expected** — 401s while logged out, 404s from IDs you made up.
- Point at the screenshots worth opening first.

```bash
kill "$(cat /tmp/frontend-walkthrough/server.pid)" 2>/dev/null
rm -f walkthrough.spec.ts walkthrough.config.ts
rm -f /tmp/frontend-walkthrough/test.db*
```

Leave the screenshots and `report.json` for the user. The spec and config must
not be committed — if a run is interrupted, delete them by hand.
