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

async function addEntry(page: Page, name: string) {
  await page.getByRole("button", { name: "Создать запись" }).click();
  await page.getByLabel("Название").fill(name);
  await page.getByLabel("Логин", { exact: true }).fill("alex");
  await page.getByLabel("Пароль", { exact: true }).fill("s3cret");
  await page.getByRole("button", { name: "Сохранить" }).click();
  const heading = page.getByRole("heading", { name, exact: true }).first();
  await expect(heading).toBeVisible();
  return heading;
}

async function headingMetrics(heading: ReturnType<Page["getByRole"]>) {
  return heading.evaluate((el) => {
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return {
      width: rect.width,
      fontSize: parseFloat(style.fontSize),
      lineHeight: parseFloat(style.lineHeight),
      lines: Math.round(rect.height / parseFloat(style.lineHeight)),
    };
  });
}

/**
 * Regression: the detail header used to collapse the title to about one
 * character on narrow screens, so "кингуру" was rendered one letter per line.
 * The title must keep a readable width; the actions wrap to their own row.
 */
test("entry detail title stays horizontal on a narrow screen", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 800 });
  await installMockTauri(page);
  await page.goto("/");
  await createVault(page);

  const heading = await addEntry(page, "кингуру");
  const metrics = await headingMetrics(heading);

  expect(metrics.width).toBeGreaterThan(metrics.fontSize * 4);
  expect(metrics.lines).toBe(1);
});

test("entry detail title wraps by words for long names", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 800 });
  await installMockTauri(page);
  await page.goto("/");
  await createVault(page);

  const heading = await addEntry(page, "Кингуру — интернет-магазин электроники");
  const metrics = await headingMetrics(heading);

  expect(metrics.width).toBeGreaterThan(metrics.fontSize * 4);
  expect(metrics.lines).toBeLessThanOrEqual(4);
});

test("ssh detail title stays horizontal on a narrow screen", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 800 });
  await installMockTauri(page);
  await page.goto("/");
  await createVault(page);

  await page.getByRole("button", { name: "SSH" }).first().click();
  await page.getByRole("button", { name: "Создать ключ" }).click();
  await page.getByLabel("Имя ключа").fill("кингуру");
  await page.getByRole("button", { name: "Сгенерировать" }).click();
  await page.getByRole("button", { name: "Сохранить" }).click();

  const heading = page.getByRole("heading", { name: "кингуру" }).first();
  await expect(heading).toBeVisible();
  const metrics = await headingMetrics(heading);

  expect(metrics.width).toBeGreaterThan(metrics.fontSize * 4);
  expect(metrics.lines).toBe(1);
});
