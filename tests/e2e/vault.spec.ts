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

test("saves a draft on manual lock and restores it after unlock", async ({ page }) => {
  await createVault(page);

  await page.getByRole("button", { name: "Создать запись" }).click();
  await page.getByLabel("Название").fill("GitHub");
  await page.getByLabel("Логин", { exact: true }).fill("alex");
  await page.getByLabel("Пароль", { exact: true }).fill("s3cret");
  await page.getByRole("button", { name: "Сохранить" }).click();
  await expect(page.getByRole("heading", { name: "GitHub" })).toBeVisible();

  await page.getByRole("button", { name: "Редактировать" }).first().click();
  await page.getByLabel("Название").fill("GitHub draft");
  const editor = page.getByRole("dialog", { name: "Редактировать запись" });
  await editor.getByRole("button", { name: "Заблокировать" }).click();

  const lockDialog = page.getByRole("dialog", { name: "Заблокировать хранилище?" });
  await expect(lockDialog).toBeVisible();
  await lockDialog.getByRole("button", { name: "Сохранить черновик" }).click();

  await expect(page.getByRole("heading", { name: "Открыть хранилище" })).toBeVisible();
  await page.getByLabel("Пароль-фраза").fill("correct horse battery staple");
  await page.getByRole("button", { name: "Разблокировать" }).click();

  await expect(page.getByText(/Несохранённый черновик: GitHub draft/)).toBeVisible();
  await page.getByRole("button", { name: "Восстановить" }).click();
  await expect(page.getByRole("dialog", { name: "Редактировать запись" })).toBeVisible();
  await expect(page.getByLabel("Название")).toHaveValue("GitHub draft");
  await page.getByRole("button", { name: "Отменить" }).click();
});

test("adds an attachment and a TOTP code", async ({ page }) => {
  await createVault(page);

  await page.getByRole("button", { name: "Создать запись" }).click();
  await page.getByLabel("Название").fill("GitHub");
  await page.getByLabel("Логин", { exact: true }).fill("alex");
  await page.getByLabel("Пароль", { exact: true }).fill("s3cret");
  await page.getByLabel("TOTP", { exact: true }).fill("JBSWY3DPEHPK3PXP");
  await page.getByLabel("TOTP", { exact: true }).press("Tab");
  await expect(page.getByText("123 456")).toBeVisible();
  await page.getByRole("button", { name: "Добавить", exact: true }).click();
  await expect(page.getByText("pms-e2e-attachment.txt")).toBeVisible();
  await page.getByRole("button", { name: "Сохранить" }).click();

  await expect(page.getByRole("heading", { name: "GitHub" })).toBeVisible();
  await expect(page.getByText("123 456")).toBeVisible();

  await page.getByRole("button", { name: /pms-e2e-attachment.txt/ }).click();
  const preview = page.getByRole("dialog", { name: "pms-e2e-attachment.txt" });
  await expect(preview).toBeVisible();
  await expect(preview.getByText("hello attachment")).toBeVisible();
  await preview.getByRole("button", { name: "Закрыть" }).click();

  await page.getByRole("button", { name: "TOTP", exact: true }).click();
  await expect(page.getByText("GitHub")).toBeVisible();
  await expect(page.getByText("123 456")).toBeVisible();
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

  await page.getByRole("button", { name: "Удалить" }).click();
  const deleteDialog = page.getByRole("dialog", { name: "Удалить запись «GitLab»?" });
  await expect(deleteDialog).toBeVisible();
  await deleteDialog.getByRole("button", { name: "Удалить" }).click();
  await expect(page.getByRole("heading", { name: "GitLab" })).toHaveCount(0);

  await page.getByRole("button", { name: /GitHub/ }).first().click();
  await page.getByRole("button", { name: "Редактировать" }).first().click();
  await page.getByLabel("Название").fill("GitHub changed");
  await page.getByRole("button", { name: "Отменить" }).click();
  const discardDialog = page.getByRole("dialog", { name: "Не сохранять изменения?" });
  await expect(discardDialog).toBeVisible();
  await discardDialog.getByRole("button", { name: "Не сохранять" }).click();
  await expect(page.getByRole("heading", { name: "GitHub changed" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "GitHub" })).toBeVisible();
});
