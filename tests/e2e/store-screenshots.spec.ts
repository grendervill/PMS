import { expect, test, type Page } from "@playwright/test";
import { installMockTauri } from "./mock-tauri";

/**
 * Screenshots for the store listing (RuStore wants portrait PNG/JPEG from
 * 1080x1920). They are generated from the real UI against the IPC mock, so
 * they stay in sync with the app after redesigns.
 *
 * Regenerate:
 *   SCREENSHOTS=1 npx playwright test tests/e2e/store-screenshots.spec.ts
 */
test.skip(!process.env.SCREENSHOTS, "run with SCREENSHOTS=1 to regenerate store screenshots");

async function createVault(page: Page) {
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

async function addEntry(
  page: Page,
  entry: { name: string; login?: string; password?: string; url?: string; comment?: string; totp?: string },
  first: boolean,
) {
  if (!first) {
    await page.getByRole("button", { name: "Назад к списку" }).click();
    await page.waitForTimeout(150);
  }
  await page.getByRole("button", { name: first ? "Создать запись" : "Запись", exact: true }).click();
  await page.getByLabel("Название").fill(entry.name);
  if (entry.login) await page.getByLabel("Логин", { exact: true }).fill(entry.login);
  if (entry.password) await page.getByLabel("Пароль", { exact: true }).fill(entry.password);
  if (entry.url) await page.getByLabel("URL", { exact: true }).fill(entry.url);
  if (entry.totp) await page.getByLabel("TOTP", { exact: true }).fill(entry.totp);
  if (entry.comment) await page.getByLabel("Комментарий").fill(entry.comment);
  await page.getByRole("button", { name: "Сохранить" }).click();
}

test.use({ viewport: { width: 540, height: 960 }, deviceScaleFactor: 2 });

test("rustore screenshots", async ({ page }) => {
  await installMockTauri(page);
  await page.goto("/");
  await createVault(page);

  // richer list rows and monograms instead of mock favicons
  await page.getByRole("button", { name: "Ещё" }).click();
  await page.getByRole("button", { name: "Настройки" }).last().click();
  await page.getByRole("button", { name: "Продвинуто" }).click();
  await page.getByLabel("Загружать значки сайтов").uncheck();
  await page.getByRole("button", { name: "Записи" }).first().click();
  await page.waitForTimeout(400);

  await addEntry(page, { name: "Кингуру", login: "alex@kenguru.ru", password: "K7!mQ2$vR9#xL", url: "https://kenguru.ru", comment: "Основной аккаунт магазина", totp: "JBSWY3DPEHPK3PXP" }, true);
  await addEntry(page, { name: "Почта", login: "alex", password: "m4il!Pass2026", url: "https://mail.example", comment: "Рабочая почта" }, false);
  await addEntry(page, { name: "Ипотека", login: "alex", password: "Bank#2026$klad", url: "https://bank.example", comment: "Платёж 12-го числа" }, false);
  await addEntry(page, { name: "Домашний Wi-Fi", login: "admin", password: "router-2026-ok", url: "http://192.168.1.1", comment: "Роутер в коридоре" }, false);
  await addEntry(page, { name: "Рабочий сервер", login: "root", password: "srv!Root#77", url: "https://server.example", comment: "Доступ по SSH-ключу" }, false);

  // 1 — список записей
  await page.getByRole("button", { name: "Назад к списку" }).click();
  await page.waitForTimeout(5000);
  await page.screenshot({ path: "docs/store/rustore/01-entries.png" });

  // 2 — карточка записи с TOTP
  await page.getByRole("button", { name: /Кингуру/ }).first().click();
  await page.waitForTimeout(1200);
  await page.screenshot({ path: "docs/store/rustore/02-entry.png" });

  // 3 — генератор пароля
  await page.getByRole("button", { name: "Ещё" }).click();
  await page.getByRole("button", { name: "Генератор" }).last().click();
  await page.waitForTimeout(800);
  await page.screenshot({ path: "docs/store/rustore/03-generator.png" });
});
