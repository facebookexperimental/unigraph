// Copyright (c) Meta Platforms, Inc. and affiliates.

import { expect, test } from "@playwright/test";

test.describe("Explorer", () => {
	test("renders the fixture graph tree", async ({ page }) => {
		await page.goto("/explorer/local");

		const explorer = page.locator(".unigraph-explorer");
		await expect(explorer).toBeVisible();
		await expect(page.getByTestId("node-row-app")).toBeVisible();
	});

	test("expands tree nodes to reveal children", async ({ page }) => {
		await page.goto("/explorer/local");

		const appRow = page.getByTestId("node-row-app");
		await expect(appRow).toBeVisible();

		// Click to select the row, then expand with ArrowRight
		await appRow.click();
		await page.keyboard.press("ArrowRight");

		// Expansion is async (wrapped in startTransition) — give it time
		await expect(page.getByTestId("node-row-ui")).toBeVisible({ timeout: 10_000 });
		await expect(page.getByTestId("node-row-core")).toBeVisible();
		await expect(page.getByTestId("node-row-utils")).toBeVisible();
	});
});
