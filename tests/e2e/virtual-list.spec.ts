import { expect, test, type Page } from "@playwright/test";
import { installMockTauri } from "./mock-tauri";

async function createVault(page: Page) {
  await page.getByRole("button", { name: "Создать хранилище" }).click();
  await page.getByRole("button", { name: "Выбрать" }).click();
  await page.getByLabel("Название хранилища").fill("Личное");
  await page.getByRole("button", { name: "Далее" }).click();
  await page.getByLabel("Пароль-фраза").fill("correct horse battery staple");
  await page.getByLabel("Повторите пароль-фразу").fill("correct horse battery staple");
  await page.getByRole("button", { name: "Далее" }).click();
  await page.getByRole("button", { name: "Создать хранилище" }).click();
}

test("a 5000 entry vault mounts only the visible cards", async ({ page }) => {
  await installMockTauri(page, { seedEntries: 5000 });
  await page.goto("/");
  await createVault(page);
  await expect(page.locator("[data-entry-id]").first()).toBeVisible();

  // Rendering all 5000 cards would mean 5000 cards and 5000 favicon fetches.
  const mounted = await page.locator("[data-entry-id]").count();
  expect(mounted).toBeGreaterThan(5);
  expect(mounted).toBeLessThan(60);

  const faviconCalls = await page.evaluate(
    () => (window as unknown as { __E2E_FAVICON_CALLS__?: string[] }).__E2E_FAVICON_CALLS__ ?? [],
  );
  expect(faviconCalls.length).toBeLessThan(60);

  const scroller = page.locator(".pms-scrollbar:has([data-entry-id])").first();
  await scroller.evaluate((element) => {
    element.scrollTop = element.scrollHeight / 2;
    element.dispatchEvent(new Event("scroll"));
  });
  await expect(page.locator("[data-entry-id='entry-seed-0']")).toHaveCount(0);
  await expect(page.locator("[data-entry-id]").first()).not.toHaveAttribute(
    "data-entry-id",
    "entry-seed-0",
  );
  expect(await page.locator("[data-entry-id]").count()).toBeLessThan(60);
});

test("a short list still renders every card", async ({ page }) => {
  await installMockTauri(page, { seedEntries: 40 });
  await page.goto("/");
  await createVault(page);
  await expect(page.locator("[data-entry-id]")).toHaveCount(40);
});
