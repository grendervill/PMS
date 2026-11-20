import { expect, test } from "@playwright/test";
import { installMockTauri } from "./mock-tauri";

test.beforeEach(async ({ page }) => {
  await installMockTauri(page, { storageMode: null });
  // Чистим localStorage один раз на тест: init-script чистил бы его и после
  // reload, из-за чего «запомненный выбор» было бы не проверить.
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
});

test("asks for the storage mode once and remembers it", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByRole("heading", { name: "Где живёт ваш сейф?" })).toBeVisible();
  await page.screenshot({ path: "test-results/storage-mode-chooser.png", fullPage: true });

  await page.getByRole("button", { name: /Работать с папкой/ }).click();
  await expect(page.getByText("Добро пожаловать")).toBeVisible();
  await page.screenshot({ path: "test-results/storage-mode-local.png", fullPage: true });

  expect(await page.evaluate(() => localStorage.getItem("pms.storageMode.v1"))).toBe("local");

  await page.reload();
  await expect(page.getByText("Добро пожаловать")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Где живёт ваш сейф?" })).toHaveCount(0);
});

test("server profile form shows the key fingerprint before login", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: /Подключить сервер/ }).click();

  await expect(page.getByRole("heading", { name: "Подключение к серверу" })).toBeVisible();
  await page.getByLabel("Адрес сервера").fill("https://pms.home:8443");
  await page.getByRole("button", { name: /Проверить сервер/ }).click();

  await expect(page.getByText("Отпечаток ключа (SHA-256)")).toBeVisible();
  await expect(page.getByRole("button", { name: /Войти/ })).toBeDisabled();
  await page.screenshot({ path: "test-results/storage-mode-server.png", fullPage: true });

  await page.getByLabel("Логин на сервере").fill("alice");
  await page.getByLabel("Пароль-фраза").fill("correct horse");
  await page.getByRole("button", { name: /Войти/ }).click();

  await expect
    .poll(async () => page.evaluate(() => localStorage.getItem("pms.storageMode.v1")))
    .toBe("remote");
  const profiles = await page.evaluate(() => localStorage.getItem("pms.profiles.v1"));
  expect(profiles).toContain("https://pms.home:8443");
  expect(profiles).toContain("\"login\":\"alice\"");
});

test("switching storage returns to the chooser", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: /Работать с папкой/ }).click();
  await expect(page.getByText("Добро пожаловать")).toBeVisible();

  await page.getByRole("button", { name: "Сменить хранилище" }).click();
  await expect(page.getByRole("heading", { name: "Где живёт ваш сейф?" })).toBeVisible();
});
