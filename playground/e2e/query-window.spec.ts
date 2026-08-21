import { test, expect } from "@playwright/test";

// The playground runs every query inside a window it never writes into the
// query text: `$__start` and `$__end` are bound by the host, the way a query
// service binds them from its time picker. The header is where a reader sees
// which window that is.

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.waitForSelector(".cm-content", { timeout: 15_000 });
});

test("the header names the window the query runs in", async ({ page }) => {
  const range = page.locator("#query-window-range");
  await expect(range).toBeVisible();
  await expect(range).toHaveText(
    /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z → \d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/,
  );
});

test("the window carries the values bound for $__start and $__end", async ({ page }) => {
  await expect(page.locator("#query-window")).toHaveAttribute(
    "title",
    /^\$__start = \d+, \$__end = \d+$/,
  );
});

test("$__start reads as a value the editor knows", async ({ page }) => {
  // The registration reaches the language server: a query that reads the
  // window back through string interpolation lints clean, while an
  // unregistered name is reported.
  const editor = page.locator(".cm-content");
  await editor.click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.press("Delete");
  await page.keyboard.insertText('test:http_requests_total\n| extend at = "${ $__start }"');
  await expect(editor).toContainText("$__start");
  await expect(page.locator(".cm-lintRange-error")).toHaveCount(0);

  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.press("Delete");
  await page.keyboard.insertText('test:http_requests_total\n| extend at = "${ $__nope }"');
  await expect(page.locator(".cm-lintRange-error")).toHaveCount(1);
});
