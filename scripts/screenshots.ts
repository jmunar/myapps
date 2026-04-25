/**
 * Automated mobile screenshot capture for the README.
 *
 * Usage:
 *   npx playwright test scripts/screenshots.ts
 *
 * Expects the server to be running at BASE_URL (default http://localhost:3000)
 * with a seeded user (username/password from env: SCREENSHOT_USER / SCREENSHOT_PASS).
 */
import { test, expect, type Page } from "@playwright/test";
import path from "path";

const BASE_URL = process.env.BASE_URL ?? "http://localhost:3000";
const USERNAME = process.env.SCREENSHOT_USER ?? "demo";
const PASSWORD = process.env.SCREENSHOT_PASS ?? "demo";

const OUT_DIR = path.resolve(__dirname, "..", "docs", "screenshots");

/** Log in and return an authenticated page. */
async function login(page: Page) {
  await page.goto(`${BASE_URL}/login`);
  await page.fill("#username", USERNAME);
  await page.fill("#password", PASSWORD);
  await page.click('button[type="submit"]');
  await page.waitForURL(`${BASE_URL}/`);
}

/** Take a viewport-sized screenshot (fixed 390x844). */
async function snap(page: Page, name: string) {
  // Wait for any HTMX swaps / animations to settle.
  await page.waitForTimeout(600);
  await page.screenshot({
    path: path.join(OUT_DIR, `${name}.png`),
  });
}

test.describe("README screenshots", () => {
  test.use({
    viewport: { width: 390, height: 844 }, // iPhone 14
    deviceScaleFactor: 3,
    isMobile: true,
    hasTouch: true,
  });

  test("capture all screens", async ({ page }) => {
    await login(page);

    // ── Launcher ──
    await snap(page, "launcher");

    // ── LeanFin ──
    await page.goto(`${BASE_URL}/leanfin`);
    await snap(page, "leanfin-transactions");

    // Open an allocation editor and click "More details" to show the raw payload
    await page.locator(".label-add-btn").first().click();
    await page.waitForSelector(".alloc-editor");
    await page.locator("button", { hasText: /More details|Más detalles/ }).first().click();
    await page.waitForSelector(".json-viewer");
    await snap(page, "leanfin-transaction-details");

    await page.goto(`${BASE_URL}/leanfin/accounts`);
    await snap(page, "leanfin-accounts");

    await page.goto(`${BASE_URL}/leanfin/balance-evolution`);
    // Wait for chart to render.
    await page.waitForTimeout(1000);
    await snap(page, "leanfin-balance");

    await page.goto(`${BASE_URL}/leanfin/expenses`);
    // Select a couple of labels so the chart is visible in the screenshot.
    const pills = page.locator(".label-pill");
    await pills.nth(0).click();
    await pills.nth(2).click();
    await page.waitForTimeout(1500);
    await snap(page, "leanfin-expenses");

    await page.goto(`${BASE_URL}/leanfin/labels`);
    await snap(page, "leanfin-labels");

    // ── MindFlow ──
    await page.goto(`${BASE_URL}/mindflow`);
    // Wait for D3 mind map to render.
    await page.waitForTimeout(1500);
    await snap(page, "mindflow-map");

    await page.goto(`${BASE_URL}/mindflow/inbox`);
    await snap(page, "mindflow-inbox");

    await page.goto(`${BASE_URL}/mindflow/actions`);
    await snap(page, "mindflow-actions");

    // ── VoiceToText ──
    await page.goto(`${BASE_URL}/voice`);
    await snap(page, "voice-to-text");

    // ── FormInput ──
    await page.goto(`${BASE_URL}/forms`);
    await snap(page, "form-input-inputs");

    await page.goto(`${BASE_URL}/forms/row-sets`);
    await snap(page, "form-input-row-sets");

    await page.goto(`${BASE_URL}/forms/form-types`);
    await snap(page, "form-input-form-types");

    // ── Notes ──
    await page.goto(`${BASE_URL}/notes`);
    await snap(page, "notes-list");

    // Open the first note for editing (click the first note link).
    const firstNote = page.locator('a[href^="/notes/"]').first();
    if (await firstNote.isVisible()) {
      await firstNote.click();
      await page.waitForTimeout(800); // Wait for editor to initialize.
      await snap(page, "notes-editor");
    }
  });
});
