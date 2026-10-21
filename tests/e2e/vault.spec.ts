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
  await page.getByLabel("URL", { exact: true }).fill("https://github.com");
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
  await preview.getByRole("button", { name: "Скачать" }).click();
  const saveDialog = page.getByRole("dialog", { name: "Сохранить файл на диск?" });
  await expect(saveDialog).toBeVisible();
  await saveDialog.getByRole("button", { name: "Сохранить" }).click();
  await expect(page.getByText("Файл сохранён")).toBeVisible();
  await preview.getByRole("button", { name: "Закрыть" }).click();

  await page.getByRole("button", { name: "TOTP", exact: true }).click();
  await expect(page.getByText("GitHub")).toBeVisible();
  await expect(page.getByText("123 456")).toBeVisible();

  await page.getByRole("button", { name: "Заблокировать" }).click();
  await page.getByLabel("Пароль-фраза").fill("correct horse battery staple");
  await page.getByRole("button", { name: "Разблокировать" }).click();
  await page.getByRole("button", { name: /GitHub/ }).first().click();
  await expect(page.getByText("pms-e2e-attachment.txt")).toBeVisible();
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

test("creates an SSH key, links it to an entry and installs it", async ({ page }) => {
  await createVault(page);

  await page.getByRole("button", { name: "SSH", exact: true }).first().click();
  await expect(page.getByRole("heading", { name: "SSH", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Создать ключ" }).click();
  await expect(page.getByRole("dialog", { name: "Новый SSH-ключ" })).toBeVisible();
  await page.getByLabel("Имя ключа").fill("GitHub key");
  await page.getByRole("button", { name: "Сгенерировать" }).click();
  await expect(page.getByText("Ключ сгенерирован")).toBeVisible();
  await page.getByRole("button", { name: "Сохранить" }).click();

  await expect(page.getByRole("heading", { name: "GitHub key" })).toBeVisible();
  await expect(page.getByText("SHA256:mockPmsSshFingerprintForE2eTests")).toBeVisible();
  await expect(page.getByText("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMockPmsSshKeyForE2eTestsOnly alex@e2e")).toBeVisible();

  await page.getByRole("button", { name: "Установить", exact: true }).first().click();
  const installDialog = page.getByRole("dialog", { name: "Установить SSH-ключ" });
  await expect(installDialog).toBeVisible();
  await installDialog.getByRole("button", { name: "Установить", exact: true }).click();
  await expect(page.getByText("Файлы установлены")).toBeVisible();
  await expect(page.getByText("/tmp/pms-home/.ssh/github-key").first()).toBeVisible();
  await installDialog.getByRole("button", { name: "Закрыть", exact: true }).last().click();

  await page.getByRole("button", { name: "Записи", exact: true }).first().click();
  await page.getByRole("button", { name: "Создать запись" }).click();
  await page.getByLabel("Название").fill("GitHub");
  await page.getByLabel("Логин", { exact: true }).fill("alex");
  await page.getByLabel("GitHub key").check();
  await page.getByRole("button", { name: "Сохранить" }).click();
  await expect(page.getByRole("heading", { name: "GitHub" })).toBeVisible();
  await expect(page.getByText("SSH-ключи")).toBeVisible();
  await expect(page.getByText("GitHub key")).toBeVisible();

  await page.getByRole("button", { name: "SSH", exact: true }).first().click();
  await page.getByRole("button", { name: /GitHub key/ }).first().click();
  await expect(page.getByText("GitHub", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "GitHub", exact: true }).click();
  await expect(page.getByRole("heading", { name: "GitHub" })).toBeVisible();

  await page.getByRole("button", { name: "Заблокировать" }).click();
  await page.getByLabel("Пароль-фраза").fill("correct horse battery staple");
  await page.getByRole("button", { name: "Разблокировать" }).click();
  await page.getByRole("button", { name: "SSH", exact: true }).first().click();
  await page.getByRole("button", { name: /GitHub key/ }).first().click();
  await expect(page.getByRole("heading", { name: "GitHub key" })).toBeVisible();
  await expect(page.getByText("SHA256:mockPmsSshFingerprintForE2eTests")).toBeVisible();
});

test("stores and searches several urls in one entry", async ({ page }) => {
  await createVault(page);

  await page.getByRole("button", { name: "Создать запись" }).click();
  await page.getByLabel("Название").fill("Multi");
  await page.getByLabel("URL", { exact: true }).fill("https://a.example");
  await page.getByRole("button", { name: "Добавить URL" }).click();
  await page.getByLabel("Дополнительный URL 2").fill("https://b.example");
  await page.getByRole("button", { name: "Сохранить" }).click();

  await expect(page.getByText("https://a.example")).toBeVisible();
  await expect(page.getByText("https://b.example")).toBeVisible();

  await page.getByLabel("Поиск по записям").fill("b.example");
  await expect(page.getByRole("button", { name: /Multi/ })).toBeVisible();
});

test("enables Touch ID, auto unlocks and falls back after five failures", async ({ page }) => {
  await createVault(page);

  await page.getByRole("button", { name: "Настройки", exact: true }).first().click();
  const toggle = page.getByRole("switch", {
    name: "Всегда разблокировать при помощи биометрии",
  });
  await expect(toggle).toBeEnabled();
  await toggle.click();
  await expect(page.getByText("Биометрия включена")).toBeVisible();
  await expect(toggle).toHaveAttribute("data-state", "checked");

  await page.evaluate(() => {
    (window as unknown as { __E2E_BIOMETRIC_FAILURES__?: number }).__E2E_BIOMETRIC_FAILURES__ = 1;
  });
  await page.getByRole("button", { name: "Заблокировать" }).click();
  await expect(page.getByText("Осталось 4 попытки")).toBeVisible();
  await page.getByRole("button", { name: "Повторить" }).click();
  await expect(page.getByRole("heading", { name: "Записи" })).toBeVisible();

  await page.evaluate(() => {
    (window as unknown as { __E2E_BIOMETRIC_FAILURES__?: number }).__E2E_BIOMETRIC_FAILURES__ = 5;
  });
  await page.getByRole("button", { name: "Заблокировать" }).click();
  await expect(page.getByText("Осталось 4 попытки")).toBeVisible();
  for (let attempt = 0; attempt < 4; attempt += 1) {
    await page.getByRole("button", { name: "Повторить" }).click();
  }
  await expect(page.getByText("Попытки закончились")).toBeVisible();
  await expect(page.getByRole("button", { name: "Войти по отпечатку" })).toHaveCount(0);

  await page.getByLabel("Пароль-фраза").fill("correct horse battery staple");
  await page.getByRole("button", { name: "Разблокировать" }).click();
  await expect(page.getByRole("heading", { name: "Записи" })).toBeVisible();

  await page.evaluate(() => {
    (window as unknown as { __E2E_BIOMETRIC_CANCEL__?: boolean }).__E2E_BIOMETRIC_CANCEL__ = true;
  });
  await page.getByRole("button", { name: "Заблокировать" }).click();
  await expect(page.getByText("Вход по отпечатку отменён")).toBeVisible();
  await expect(page.getByRole("button", { name: "Войти по отпечатку" })).toHaveCount(0);
  await page.evaluate(() => {
    (window as unknown as { __E2E_BIOMETRIC_CANCEL__?: boolean }).__E2E_BIOMETRIC_CANCEL__ = false;
  });
  await page.getByLabel("Пароль-фраза").fill("correct horse battery staple");
  await page.getByRole("button", { name: "Разблокировать" }).click();
  await expect(page.getByRole("heading", { name: "Записи" })).toBeVisible();
});

test("shares a card by QR and shows a TOTP QR", async ({ page }) => {
  await createVault(page);

  await page.getByRole("button", { name: "Создать запись" }).click();
  await page.getByLabel("Название").fill("Share me");
  await page.getByLabel("Логин", { exact: true }).fill("alex");
  await page.getByLabel("Пароль", { exact: true }).fill("s3cret");
  await page.getByLabel("URL", { exact: true }).fill("https://shared.example");
  await page.getByLabel("TOTP", { exact: true }).fill("JBSWY3DPEHPK3PXP");
  await page.getByLabel("TOTP", { exact: true }).press("Tab");
  await page.getByRole("button", { name: "Сохранить" }).click();

  await page.getByRole("button", { name: "Поделиться по QR" }).click();
  await expect(page.getByRole("dialog", { name: "Поделиться карточкой" })).toBeVisible();
  await expect(page.getByAltText("QR-код карточки")).toBeVisible();
  await expect(page.getByText(/данные карточки лежат в QR открыто/)).toBeVisible();
  await page.getByRole("button", { name: "С паролем" }).click();
  const createQr = page.getByRole("button", { name: "Создать QR" });
  await expect(createQr).toBeDisabled();
  await page.getByLabel("Пароль QR").fill("qr-pass");
  await expect(createQr).toBeEnabled();
  await createQr.click();
  await expect(page.getByAltText("QR-код карточки")).toBeVisible();
  await page.getByRole("button", { name: "Закрыть", exact: true }).last().click();

  await page.getByRole("button", { name: "Показать TOTP QR" }).click();
  await expect(page.getByRole("dialog", { name: "TOTP QR" })).toBeVisible();
  await expect(page.getByAltText("TOTP QR")).toBeVisible();
  await page.getByRole("button", { name: "Закрыть", exact: true }).last().click();

  await page.getByRole("button", { name: "Добавить по QR" }).click();
  await expect(page.getByRole("dialog", { name: "Импорт карточки по QR" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Изображение", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Камера" })).toBeVisible();
});

test("imports a manager file and exports a pmsvault backup", async ({ page }) => {
  await createVault(page);

  await page.getByRole("button", { name: "Настройки", exact: true }).first().click();
  await page.getByRole("button", { name: "Импорт из менеджера" }).click();
  await expect(page.getByRole("dialog", { name: "Импорт из другого менеджера" })).toBeVisible();
  await page.getByRole("button", { name: "Выбрать", exact: true }).click();
  await page.getByRole("button", { name: "Проверить файл" }).click();
  await expect(page.getByText("Формат: csv")).toBeVisible();
  await expect(page.getByText(/Записей: 1/)).toBeVisible();
  await page.getByRole("button", { name: "Импортировать" }).click();
  await expect(page.getByText("Импорт завершён")).toBeVisible();
  await page.getByRole("button", { name: "Закрыть", exact: true }).last().click();

  await page.getByRole("button", { name: "Записи", exact: true }).first().click();
  await expect(page.getByText("Imported")).toBeVisible();
  await page.getByRole("button", { name: /Imported/ }).first().click();
  await expect(page.getByText("note.txt")).toBeVisible();

  await page.getByRole("button", { name: "Настройки", exact: true }).first().click();
  await page.getByRole("button", { name: "Экспорт .pmsvault" }).click();
  await expect(page.getByText("Резервная копия создана")).toBeVisible();
});

test("selects an application icon", async ({ page }) => {
  await createVault(page);
  await page.getByRole("button", { name: "Настройки", exact: true }).first().click();
  await expect(page.getByRole("button", { name: "Зелёный замок" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.getByRole("button", { name: "Синий щит" }).click();
  await expect(page.getByRole("button", { name: "Синий щит" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByText("Значок приложения обновлён")).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("pms.appIcon")))
    .toBe("blue-shield");
});
