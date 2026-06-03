import { test, expect, type Page } from "@playwright/test";

// Faithful reproduction of the reported failure: the tag-vs-tag EXAMPLE has a
// 3-line `//` comment header, and completion must work both while typing
// (auto-trigger) and on Ctrl+Space, in the default (vim) editor mode.
const COMMENTED = [
  "// Tag-vs-tag comparison: a tag can be referenced as a value, so the RHS of a",
  "// filter can be another tag instead of a constant. Here we keep only",
  "// cross-region traffic (source and destination differ) and total it.",
  "service_mesh:mesh_request_count",
  "| where src_region != ",
].join("\n");

async function loadPrefix(page: Page) {
  const editor = page.locator(".cm-content");
  await editor.click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.press("Delete");
  await page.keyboard.insertText(COMMENTED);
  // confirm the multi-line commented preamble actually landed
  await expect(editor).toContainText("Tag-vs-tag comparison");
  await expect(editor).toContainText("service_mesh:mesh_request_count");
}

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.waitForSelector(".cm-content", { timeout: 15_000 });
});

test("commented example: typing a tag AUTO-triggers the popup (dst_region)", async ({ page }) => {
  await loadPrefix(page);
  await page.keyboard.type("d", { delay: 60 }); // real keystroke, NO Ctrl+Space
  const tip = page.locator(".cm-tooltip-autocomplete");
  await expect(tip).toBeVisible({ timeout: 5000 });
  await expect(tip.getByText("dst_region", { exact: false })).toBeVisible();
});

test("commented example: Ctrl+Space shows the popup (dst_region)", async ({ page }) => {
  await loadPrefix(page);
  await page.keyboard.type("d", { delay: 60 });
  await page.keyboard.press("Control+Space");
  const tip = page.locator(".cm-tooltip-autocomplete");
  await expect(tip).toBeVisible({ timeout: 5000 });
  await expect(tip.getByText("dst_region", { exact: false })).toBeVisible();
});
