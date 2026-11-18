import { expect, test, type Page } from "@playwright/test";
import { installMockTauri } from "./mock-tauri";

async function openVault(page: Page) {
  await page.getByRole("button", { name: "Создать хранилище" }).click();
  await page.getByRole("button", { name: "Выбрать" }).click();
  await page.getByLabel("Название хранилища").fill("Личное");
  await page.getByRole("button", { name: "Далее" }).click();
  await page.getByLabel("Пароль-фраза").fill("correct horse battery staple");
  await page.getByLabel("Повторите пароль-фразу").fill("correct horse battery staple");
  await page.getByRole("button", { name: "Далее" }).click();
  await page.getByRole("button", { name: "Создать хранилище" }).click();
  await expect(page.getByRole("heading", { name: "Записи" })).toBeVisible();
}

test("imports a custom word list and switches the passphrase to it", async ({ page }) => {
  await installMockTauri(page);
  await page.goto("/");
  await openVault(page);

  await page.getByRole("button", { name: "Генератор" }).click();
  await page.getByRole("button", { name: "Парольная фраза" }).click();

  // The built-in list is selected until a custom one is imported.
  await expect(page.getByLabel("Словник", { exact: true })).toHaveValue("eff");
  await page.getByRole("button", { name: /Импорт/ }).click();

  await expect(page.getByLabel("Словник", { exact: true })).toHaveValue("custom");
  await expect(page.getByRole("option", { name: /Свой, 900 слов/ })).toBeAttached();
  // The number position only appears when a digit is requested.
  await expect(page.getByLabel("Где цифра")).toHaveCount(0);
  await page.getByRole("switch", { name: /Добавить цифру/ }).click();
  await expect(page.getByLabel("Где цифра")).toHaveValue("end");
});
