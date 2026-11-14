import { expect, test } from "@playwright/test";

test("advanced fields stay optional and invalid requests cannot be submitted", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Nueva descarga", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("button", { name: "Añadir descarga", exact: true })).toBeDisabled();
  await expect(dialog.locator('[name="threads"]')).toHaveCount(0);
  await dialog.locator('[name="request"]').fill("https://example.com/flow-check.zip");
  await dialog.getByRole("button", { name: "Configuración avanzada" }).click();
  await expect(dialog.locator('[name="threads"]')).toBeVisible();
  await dialog.locator('[name="single-stream"]').selectOption({ label: "Una solicitud continua" });
  await dialog.getByRole("button", { name: "Añadir descarga", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText("flow-check.zip", { exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByText("flow-check.zip", { exact: true })).toBeVisible();
});

test("restarting a non-resumable partial displays a visible confirmation with trapped focus", async ({
  page,
}) => {
  await page.goto("/");
  await page.locator(".download-row").first().waitFor();
  // The browser gateway is intentionally a demo. Seed a failed partial to
  // exercise recovery UX without pretending that this is a real transfer.
  await page.evaluate(() => {
    const state = JSON.parse(localStorage.getItem("fluxor.preview.snapshot.v2")!);
    const item = state.downloads[0];
    item.fileName = "recovery-check.zip";
    item.state = { kind: "failed", message: "Connection lost", recoverable: true };
    item.transfer = {
      downloadedBytes: 999,
      size: { kind: "known", totalBytes: 1000 },
      validator: { kind: "none" },
      resume: { kind: "unsupported", reason: "No ranges" },
    };
    state.downloads = [item];
    localStorage.setItem("fluxor.preview.snapshot.v2", JSON.stringify(state));
  });
  await page.reload();
  await expect(page.getByText("99%", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Reiniciar desde cero", exact: true }).click();
  const confirmation = page.getByRole("alertdialog");
  await expect(confirmation).toBeVisible();
  await expect(confirmation).toHaveCSS("opacity", "1");
  const cancel = confirmation.getByRole("button", { name: "Cancelar", exact: true });
  await expect(cancel).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(confirmation.getByRole("button", { name: "Reiniciar desde cero" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(confirmation).toHaveCount(0);
  await expect(page.getByText("99%", { exact: true })).toBeVisible();
});
