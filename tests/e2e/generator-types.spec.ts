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

async function openGenerator(page: Page) {
  await installMockTauri(page);
  await page.goto("/");
  await openVault(page);
  await page.getByRole("button", { name: "Генератор" }).click();
  await expect(page.getByTestId("generated-value")).toBeVisible();
}

test("switches between the random, readable and PIN password types", async ({ page }) => {
  await openGenerator(page);

  // The type switch lives inside the "Пароль" mode.
  await expect(page.getByRole("group", { name: "Тип пароля" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Случайный" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  await page.getByRole("button", { name: "Читаемый" }).click();
  await expect(page.getByLabel("Длина читаемого пароля", { exact: true })).toHaveValue("16");
  await expect(page.getByRole("switch", { name: /С заглавной буквы/ })).not.toBeChecked();
  await expect(page.getByTestId("generated-value")).toHaveText(/^[a-z]+$/);
  await page.getByRole("switch", { name: /С заглавной буквы/ }).click();
  await page.getByRole("button", { name: "Сгенерировать", exact: true }).click();
  await expect(page.getByTestId("generated-value")).toHaveText(/^[A-Z][a-z]+$/);

  await page.getByRole("button", { name: "ПИН-код" }).click();
  await expect(page.getByLabel("Длина ПИН-кода", { exact: true })).toHaveValue("6");
  await expect(page.getByRole("switch", { name: /Исключать 1111 и 1234/ })).toBeChecked();
  await expect(page.getByText(/ПИН-код слабее пароля/)).toBeVisible();
  await expect(page.getByTestId("generated-value")).toHaveText(/^\d{6}$/);
  // No strength bar or "too weak" block for a PIN.
  await expect(page.getByText("Слишком слабо — увеличьте длину")).toHaveCount(0);

  // Back to a random password: the charset controls return.
  await page.getByRole("button", { name: "Случайный" }).click();
  await expect(page.getByLabel("Исключить символы")).toBeVisible();
  await expect(page.getByTestId("generated-value")).toHaveText(/^[!-~]+$/);

  // The passphrase mode keeps working next to the new type switch.
  await page.getByRole("button", { name: "Парольная фраза" }).click();
  await expect(page.getByLabel("Количество слов", { exact: true })).toHaveValue("5");
  await expect(page.getByRole("group", { name: "Тип пароля" })).toHaveCount(0);
  await expect(page.getByTestId("generated-value")).toHaveText(/^[a-z-]+$/);
});

test("checks the generated value against known breaches", async ({ page }) => {
  await openGenerator(page);

  await page.getByRole("button", { name: /Проверить утечки/ }).click();
  await expect(page.getByText("В известных утечках не найден")).toBeVisible();

  // The check is not offered for logins.
  await page.getByRole("button", { name: "Логин", exact: true }).click();
  await expect(page.getByRole("button", { name: /Проверить утечки/ })).toHaveCount(0);
});

/** The switches and buttons animate for ~150 ms; wait so shots show the end state. */
async function shot(page: Page, path: string) {
  await page.waitForTimeout(300);
  await page.screenshot({ path, fullPage: true });
}

test("captures the generator at 1280 and 360 pixels", async ({ page }) => {
  await openGenerator(page);

  await shot(page, "/tmp/gen2-1280-random.png");

  await page.getByRole("button", { name: "Читаемый" }).click();
  await shot(page, "/tmp/gen2-1280-readable.png");

  await page.getByRole("button", { name: "ПИН-код" }).click();
  await shot(page, "/tmp/gen2-1280-pin.png");

  await page.getByRole("button", { name: "Парольная фраза" }).click();
  await shot(page, "/tmp/gen2-1280-passphrase.png");

  await page.setViewportSize({ width: 360, height: 800 });
  await page.getByRole("button", { name: "Пароль", exact: true }).click();
  await shot(page, "/tmp/gen2-360-random.png");

  await page.getByRole("button", { name: "Читаемый" }).click();
  await shot(page, "/tmp/gen2-360-readable.png");

  await page.getByRole("button", { name: "ПИН-код" }).click();
  await shot(page, "/tmp/gen2-360-pin.png");

  await page.getByRole("button", { name: "Парольная фраза" }).click();
  await shot(page, "/tmp/gen2-360-passphrase.png");
});
