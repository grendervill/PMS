import { expect, test } from "@playwright/test";

/**
 * Визуальная проверка серверных экранов (раздел квоты и диалог конфликта) без
 * настоящего сервера: dev-витрина `?preview=sync`.
 */
test("server section and conflict dialog render", async ({ page }) => {
  await page.goto("/?preview=sync");

  await expect(page.getByRole("heading", { name: "Серверное хранилище" }).first()).toBeVisible();
  await expect(page.getByRole("progressbar").first()).toBeVisible();
  await page.screenshot({ path: "test-results/sync-settings.png", fullPage: true });

  await page.getByRole("button", { name: "Удалить аккаунт на сервере" }).first().click();
  await expect(page.getByText("Сейф изменён на другом устройстве")).toBeVisible();
  await page.screenshot({ path: "test-results/sync-conflict.png", fullPage: true });
});
