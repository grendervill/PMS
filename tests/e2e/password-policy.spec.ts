import { expect, test, type Page } from "@playwright/test";
import { installMockTauri } from "./mock-tauri";

const DAY = 86_400_000;

const policy = {
  minLength: 12,
  maxLength: 20,
  requireLowercase: true,
  requireUppercase: false,
  requireDigits: true,
  requireSymbols: false,
  forbiddenChars: "<>",
};

/** A vault with one entry whose password has not been touched for 214 days. */
const staleEntry = {
  id: "entry-stale",
  name: "Банк",
  login: "alex@example.com",
  password: "OldPassw0rd!",
  url: "https://bank.example",
  urls: ["https://bank.example"],
  comment: "Личный кабинет",
  policy,
  createdAt: new Date(Date.now() - 400 * DAY).toISOString(),
  updatedAt: new Date(Date.now() - 214 * DAY).toISOString(),
};

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

async function fillEntryPolicy(page: Page) {
  await page.getByRole("button", { name: "Запись", exact: true }).click();
  await page.getByLabel("Название").fill("Банк");
  await page.getByRole("textbox", { name: "Логин" }).fill("alex@example.com");
  await page.getByRole("button", { name: /Правила пароля на сайте/ }).click();
  await page.getByLabel("Минимальная длина").fill("12");
  await page.getByLabel("Максимальная длина").fill("20");
  await page.getByLabel("Строчные буквы").check();
  await page.getByLabel("Цифры").check();
  await page.getByLabel("Запрещённые символы").fill("<>");
  await expect(page.getByRole("button", { name: /Правила пароля на сайте/ })).toContainText(
    "длина 12–20",
  );
}

test("the editor hands the site policy to the generator", async ({ page }) => {
  await installMockTauri(page);
  await page.goto("/");
  await openVault(page);
  await fillEntryPolicy(page);

  await page.waitForTimeout(300);
  await page.screenshot({ path: "/tmp/policy-editor.png", fullPage: false });
  await page.setViewportSize({ width: 360, height: 800 });
  await page.waitForTimeout(300);
  await page.screenshot({ path: "/tmp/policy-editor-360.png", fullPage: false });
  await page.setViewportSize({ width: 1280, height: 900 });

  await page.getByRole("button", { name: "Сгенерировать пароль" }).click();
  await expect(page.getByTestId("generator-policy-note")).toBeVisible();

  // Length is clamped into the site range, the required sets are on and locked,
  // and the banned characters become the exclusion list.
  await expect(page.getByLabel("Длина", { exact: true })).toHaveValue("16");
  await expect(page.getByRole("switch", { name: "Цифры" })).toBeChecked();
  await expect(page.getByRole("switch", { name: "Цифры" })).toBeDisabled();
  // A set the site does not require keeps its default and stays editable.
  await expect(page.getByRole("switch", { name: "Заглавные" })).toBeChecked();
  await expect(page.getByRole("switch", { name: "Заглавные" })).toBeEnabled();
  await expect(page.getByLabel("Исключить символы")).toHaveValue("<>");
  await expect(page.getByTestId("generator-policy-note")).toHaveText(
    "Учтены правила сайта: длина 12–20 · обязательны: строчные буквы, цифры · запрещены <>",
  );
  await expect(page.getByRole("button", { name: "Длина 8" })).toHaveCount(0);

  await page.waitForTimeout(300);
  await page.screenshot({ path: "/tmp/policy-gen.png", fullPage: false });

  // The exclusion list carries the banned characters; scroll it into view.
  await page.getByLabel("Исключить символы").scrollIntoViewIfNeeded();
  await page.waitForTimeout(300);
  await page.screenshot({ path: "/tmp/policy-gen-exclude.png", fullPage: false });
  await page.getByTestId("generator-policy-note").scrollIntoViewIfNeeded();

  await page.setViewportSize({ width: 360, height: 800 });
  await page.waitForTimeout(300);
  await page.screenshot({ path: "/tmp/policy-gen-360.png", fullPage: false });
});

test("the detail view asks for a new password after 180 days", async ({ page }) => {
  await installMockTauri(page, { seedExtraEntries: [staleEntry] });
  await page.goto("/");
  await openVault(page);

  // The card list already marks the entry: rules known, password old.
  const card = page.getByRole("button", { name: /Банк/ }).first();
  await expect(card.getByLabel("Заданы правила пароля")).toBeVisible();
  await expect(card.getByLabel("Пароль старше 180 дней")).toBeVisible();
  await card.click();

  await expect(page.getByText("Пароль не менялся 214 дней — сгенерировать новый?")).toBeVisible();
  await expect(page.getByText("Правила сайта")).toBeVisible();

  await page.waitForTimeout(300);
  await page.screenshot({ path: "/tmp/policy-rotation.png", fullPage: false });

  await page.setViewportSize({ width: 360, height: 800 });
  await page.waitForTimeout(300);
  await page.screenshot({ path: "/tmp/policy-rotation-360.png", fullPage: false });

  // The button opens the generator with the stored rules.
  await page.getByRole("button", { name: "Сгенерировать", exact: true }).click();
  await expect(page.getByTestId("generator-policy-note")).toContainText("длина 12–20");
  await expect(page.getByTestId("generated-value")).not.toBeEmpty();

  // Using the value replaces the password, so the reminder goes away.
  await page.getByRole("button", { name: "Использовать" }).click();
  await expect(page.getByText("Пароль обновлён")).toBeVisible();
  await expect(page.getByText(/Пароль не менялся/)).toHaveCount(0);
});
