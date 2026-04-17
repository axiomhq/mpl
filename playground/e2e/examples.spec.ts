import { test, expect } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";

const examplesDir = path.resolve(import.meta.dirname, "../examples");
const examples = fs
  .readdirSync(examplesDir)
  .filter((f) => f.endsWith(".mpl"))
  .sort();

// Examples that produce known errors (unsupported features).
const expectedErrors: Record<string, string> = {
  "rate.mpl": "Compute",
  "replace_labels.mpl": "not supported",
  "slo-ingest-rate.mpl": "Compute",
  "enrich.mpl": "not supported",
  "nested-enrich.mpl": "not supported",
  "params.mpl": "Parameterized",
  "filtered-histogram.mpl": "Unknown tag",
};

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  // Wait for Alpine + WASM + datasets to initialize
  await page.waitForSelector(".chart-label", { timeout: 10_000 });
});

for (const example of examples) {
  test(`${example} renders without crash`, async ({ page }) => {
    // Select the example from the dropdown
    const select = page.locator("#example-select");
    await select.selectOption({ label: example });

    // Wait for charts to re-render after example change
    await page.waitForTimeout(500);
    await page.waitForSelector(".chart-label", { timeout: 5000 });

    const labels = page.locator(".chart-label");
    await expect(labels.first()).toBeVisible();

    const expectedError = expectedErrors[example];
    if (expectedError) {
      const errors = page.locator(".chart-error");
      await expect(errors.first()).toBeVisible();
    } else {
      // Should show at least one chart (canvas) and no errors
      const charts = page.locator(".chart-widget canvas");
      await expect(charts.first()).toBeVisible({ timeout: 5000 });
      const errors = page.locator(".chart-error");
      expect(await errors.count()).toBe(0);
    }
  });
}
