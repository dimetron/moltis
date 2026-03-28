const { test, expect } = require("@playwright/test");
const { navigateAndWait, watchPageErrors } = require("../helpers");

test.describe("SSH settings", () => {
	test("can generate a deploy key and add a managed SSH target", async ({ page }) => {
		const pageErrors = watchPageErrors(page);
		await navigateAndWait(page, "/settings/ssh");

		const suffix = Date.now().toString().slice(-6);
		const keyName = `e2e-key-${suffix}`;
		const targetLabel = `e2e-target-${suffix}`;

		await page.getByPlaceholder("production-box").fill(keyName);
		await page.getByRole("button", { name: "Generate", exact: true }).click();

		await expect(page.locator(".provider-item-name", { hasText: keyName }).first()).toBeVisible({
			timeout: 15_000,
		});
		await expect(page.getByRole("button", { name: "Copy Public Key", exact: true }).first()).toBeVisible();

		await page.getByPlaceholder("prod-box").fill(targetLabel);
		await page.getByPlaceholder("deploy@example.com").fill("deploy@example.com");
		await page.locator("select").nth(0).selectOption("managed");
		await page.locator("select").nth(1).selectOption({ label: keyName });
		await page.getByRole("button", { name: "Add Target", exact: true }).click();

		const targetCard = page.locator(".provider-item", { hasText: targetLabel }).first();
		await expect(targetCard).toBeVisible({ timeout: 15_000 });
		await expect(targetCard.getByText("Managed key", { exact: true })).toBeVisible();

		expect(pageErrors).toEqual([]);
	});
});
