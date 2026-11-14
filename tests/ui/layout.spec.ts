import { expect, test, type Page } from "@playwright/test";

async function expectFits(page: Page) {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  const dialog = page.getByRole("dialog");
  if (!(await dialog.count())) return;
  const viewport = page.viewportSize()!;
  const box = (await dialog.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(-1);
  expect(box.x + box.width).toBeLessThanOrEqual(viewport.width + 1);
  for (const button of await dialog.locator("footer button").all()) {
    if (!(await button.isVisible())) continue;
    const bounds = (await button.boundingBox())!;
    expect(bounds.y).toBeGreaterThanOrEqual(0);
    expect(bounds.y + bounds.height).toBeLessThanOrEqual(viewport.height + 1);
  }
}

for (const width of [320, 390, 768, 1024, 1440]) {
  test(`forms and recovery controls remain reachable at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/");
    await page.getByRole("button", { name: "Nueva descarga", exact: true }).click();
    await page.getByRole("button", { name: "Configuración avanzada" }).click();
    await expectFits(page);
    await page.getByRole("button", { name: "Cancelar", exact: true }).click();
    await page.getByRole("button", { name: "Preferencias", exact: true }).first().click();
    await expectFits(page);
    await page.getByRole("button", { name: /Carpetas/ }).click();
    await expectFits(page);
    await page.getByRole("button", { name: /Diagnóstico/ }).click();
    await expectFits(page);
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "Cerrar", exact: true })
      .last()
      .click();
    await page.getByRole("button", { name: "Perfiles proxy", exact: true }).first().click();
    await expectFits(page);
    await page.getByRole("button", { name: "Cerrar", exact: true }).click();
    await page.locator(".download-row").first().click();
    await expect(page.locator(".detail-panel")).toBeVisible();
    await expectFits(page);
  });
}

test("dialog actions fit in a short desktop window", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 600 });
  await page.goto("/");
  await page.getByRole("button", { name: "Preferencias", exact: true }).first().click();
  await expectFits(page);
  await page.getByRole("button", { name: "Guardar cambios", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
});
