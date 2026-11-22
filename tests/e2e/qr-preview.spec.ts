import { expect, test } from "@playwright/test";

/** Телефонный вьюпорт: так видно, обрезается ли QR и кадр камеры. */
test.use({
  viewport: { width: 412, height: 915 },
  launchOptions: {
    args: ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"],
  },
});

test("scanner camera and TOTP QR fit the phone screen", async ({ page }) => {
  await page.goto("/?preview=qr");

  await page.getByRole("button", { name: "Открыть сканер" }).click();
  await page.getByRole("button", { name: /Камера/ }).click();
  await expect(page.getByText(/Сканирование происходит локально/)).toBeVisible();
  await page.waitForTimeout(1200);

  // Видео должно занимать весь кадр: раньше оно было 16:9 внутри рамки 4:5,
  // и нижняя половина кадра оставалась чёрной — наводить камеру было некуда.
  const video = page.locator("video");
  const frame = await video.boundingBox();
  const container = await video.locator("xpath=..").boundingBox();
  expect(frame, "видео не отрисовано").not.toBeNull();
  expect(container, "кадр не отрисован").not.toBeNull();
  expect(frame!.height).toBeGreaterThan(container!.height - 4);
  expect(frame!.width).toBeGreaterThan(container!.width - 4);

  await page.screenshot({ path: "test-results/qr-scanner-phone.png", fullPage: false });

  await page.getByRole("button", { name: "Закрыть", exact: true }).last().click();
  await page.getByRole("button", { name: "Показать TOTP QR" }).click();
  await expect(page.getByAltText("TOTP QR")).toBeVisible();
  await page.screenshot({ path: "test-results/qr-totp-phone.png", fullPage: false });
});
