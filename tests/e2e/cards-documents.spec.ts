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

async function openSection(page: Page, name: string) {
  await page.getByRole("button", { name, exact: true }).first().click();
}

test.beforeEach(async ({ page }) => {
  await installMockTauri(page);
  await page.goto("/");
});

test("creates a bank card, opens it and flips to CVV", async ({ page }) => {
  await createVault(page);
  await openSection(page, "Карты");
  await page.getByRole("button", { name: "Добавить карту" }).click();

  await expect(page.getByText("Новая карта")).toBeVisible();
  await page.getByLabel("Номер карты").fill("4111111111111111");
  await page.getByLabel("Срок действия").fill("12/29");
  await page.getByLabel("CVV").fill("123");
  await page.getByLabel("Имя владельца").fill("IVAN IVANOV");
  await page.getByRole("button", { name: "Основная" }).click();
  await page.getByRole("button", { name: "Сохранить" }).click();

  await expect(page.getByText("Основная · 1111")).toBeVisible();
  await expect(page.getByRole("button", { name: "Показать обратную сторону" })).toBeVisible();
  await page.getByRole("button", { name: "Показать обратную сторону" }).click();
  await expect(page.getByRole("button", { name: "Показать лицевую сторону" })).toBeVisible();
  await expect(page.getByText("123").last()).toBeVisible();
  await expect(page.getByText("12/29").last()).toBeVisible();
});

test("creates passport, SNILS and driver's license", async ({ page }) => {
  await createVault(page);
  await openSection(page, "Документы");
  await page.getByRole("button", { name: "Добавить документ" }).click();

  await page.getByLabel("Серия").fill("4509");
  await page.getByLabel("Номер", { exact: true }).fill("123456");
  await page.getByLabel("Кем выдан").fill("ОВД района");
  await page.getByLabel("Код подразделения").fill("770001");
  await page.getByLabel("Дата выдачи").fill("2020-01-01");
  await page.getByLabel("Адрес прописки").fill("Москва");
  await page.getByRole("button", { name: "Сохранить" }).click();

  await expect(page.getByRole("dialog").getByText("Паспорт РФ")).toBeVisible();
  await page.getByRole("button", { name: "Закрыть", exact: true }).click();

  await page.getByRole("button", { name: "Добавить документ" }).click();
  await page.getByRole("button", { name: /^СНИЛС/ }).click();
  await page.getByLabel("Номер СНИЛС").fill("11223344595");
  await page.getByRole("button", { name: "Сохранить" }).click();
  await expect(page.getByRole("dialog").getByRole("heading", { name: "СНИЛС" })).toBeVisible();
  await page.getByRole("button", { name: "Закрыть", exact: true }).click();
  await expect(page.getByRole("button", { name: "СНИЛС · 4595" })).toBeVisible();

  await page.getByRole("button", { name: "Добавить документ" }).click();
  await page.getByRole("button", { name: /^Водительское удостоверение/ }).click();
  await page.getByLabel("ФИО").fill("ИВАНОВ ИВАН");
  await page.getByLabel("Серия").fill("9900");
  await page.getByLabel("Номер", { exact: true }).fill("123456");
  await page.getByLabel("Кем выдан").fill("Госавтоинспекция");
  await page.getByLabel("Дата выдачи").fill("2020-01-01");
  await page.getByLabel("Действует до").fill("2030-01-01");
  await page.getByLabel("Категории").fill("B, M");
  await page.getByRole("button", { name: "Сохранить" }).click();
  await expect(
    page.getByRole("dialog").getByRole("heading", { name: "Водительское удостоверение" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Закрыть", exact: true }).click();
  await expect(
    page.getByRole("button", { name: /Водительское удостоверение · 3456/ }),
  ).toBeVisible();
});

test("attaches a scan and exposes cards/documents on mobile", async ({ page }) => {
  await createVault(page);
  await openSection(page, "Документы");
  await page.getByRole("button", { name: "Добавить документ" }).click();
  await page.getByLabel("Серия").fill("4509");
  await page.getByLabel("Номер", { exact: true }).fill("123456");
  await page.getByLabel("Кем выдан").fill("ОВД района");
  await page.getByLabel("Код подразделения").fill("770001");
  await page.getByLabel("Дата выдачи").fill("2020-01-01");
  await page.getByLabel("Адрес прописки").fill("Москва");
  await page.getByRole("button", { name: "Добавить" }).click();
  await expect(page.getByText("pms-e2e-attachment.txt")).toBeVisible();
  await page.getByRole("button", { name: "Сохранить" }).click();
  await page.getByRole("button", { name: "Закрыть", exact: true }).click();
  await page.getByRole("button", { name: "Паспорт РФ · 3456" }).click();
  await expect(page.getByText("pms-e2e-attachment.txt")).toBeVisible();
  await page.getByRole("button", { name: "Закрыть", exact: true }).click();

  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "Ещё" }).click();
  await expect(page.getByRole("button", { name: "Карты" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Документы" })).toBeVisible();
});

test("accepts a space in the cardholder name while typing", async ({ page }) => {
  await createVault(page);
  await openSection(page, "Карты");
  await page.getByRole("button", { name: "Добавить карту" }).click();

  const holder = page.getByLabel("Имя владельца");
  await holder.fill("IVAN");
  await holder.press("Space");
  await expect(holder).toHaveValue("IVAN ");
  await holder.type("IVANOV");
  await expect(holder).toHaveValue("IVAN IVANOV");
});
