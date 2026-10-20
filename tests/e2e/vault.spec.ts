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
  await expect(page.getByRole("heading", { name: "Записи" })).toBeVisible();
}

test.beforeEach(async ({ page }) => {
  await installMockTauri(page);
  await page.goto("/");
});

test("creates a vault, adds an entry, generates a password and unlocks again", async ({
  page,
}) => {
  await createVault(page);

  await page.getByRole("button", { name: "Создать запись" }).click();
  await page.getByLabel("Название").fill("GitHub");
  await page.getByLabel("Логин", { exact: true }).fill("alex");
  await page.getByLabel("URL").fill("https://github.com");
  await page.getByLabel("Комментарий").fill("Личный аккаунт");

  await page.getByRole("button", { name: "Сгенерировать пароль" }).click();
  await expect(page.getByRole("dialog", { name: "Генератор пароля" })).toBeVisible();
  await expect(page.getByText("A7!kP9#vQ2$mR5@x")).toBeVisible();
  await page.getByRole("button", { name: "Использовать" }).click();

  await page.getByRole("button", { name: "Сохранить" }).click();
  await expect(page.getByRole("heading", { name: "GitHub" })).toBeVisible();
  await expect(
    page.getByRole("paragraph").filter({ hasText: "Личный аккаунт" }),
  ).toBeVisible();

  await page.getByRole("button", { name: "Заблокировать" }).click();
  await expect(page.getByRole("heading", { name: "Открыть хранилище" })).toBeVisible();
  await page.getByLabel("Пароль-фраза").fill("correct horse battery staple");
  await page.getByRole("button", { name: "Разблокировать" }).click();

  await page.getByRole("button", { name: /GitHub/ }).first().click();
  await expect(page.getByRole("heading", { name: "GitHub" })).toBeVisible();
});

test("blocks reusing a historical password and warns about duplicates", async ({ page }) => {
  await createVault(page);

  await page.getByRole("button", { name: "Создать запись" }).click();
  await page.getByLabel("Название").fill("GitHub");
  await page.getByLabel("Логин", { exact: true }).fill("alex");
  await page.getByLabel("Пароль", { exact: true }).fill("s3cret");
  await page.getByRole("button", { name: "Сохранить" }).click();
  await expect(page.getByRole("heading", { name: "GitHub" })).toBeVisible();

  await page.getByRole("button", { name: "Редактировать" }).first().click();
  await page.getByLabel("Пароль", { exact: true }).fill("new-secret");
  await page.getByRole("button", { name: "Сохранить" }).click();
  await expect(page.getByRole("heading", { name: "GitHub" })).toBeVisible();

  await page.getByRole("button", { name: "Редактировать" }).first().click();
  await page.getByLabel("Пароль", { exact: true }).fill("s3cret");
  await page.getByRole("button", { name: "Сохранить" }).click();
  await expect(
    page.getByText("Этот пароль уже использовался в этой записи. Выберите другой."),
  ).toBeVisible();

  await page.getByLabel("Пароль", { exact: true }).fill("duplicate-secret");
  await page.getByRole("button", { name: "Сохранить" }).click();
  await expect(page.getByRole("heading", { name: "GitHub" })).toBeVisible();

  await page.getByRole("button", { name: "Запись", exact: true }).click();
  await page.getByLabel("Название").fill("GitLab");
  await page.getByLabel("Пароль", { exact: true }).fill("duplicate-secret");
  await expect(page.getByText(/Этот пароль уже используется в:/)).toBeVisible();
  await page.getByRole("button", { name: "Сохранить" }).click();
  await expect(page.getByRole("heading", { name: "GitLab" })).toBeVisible();
});
