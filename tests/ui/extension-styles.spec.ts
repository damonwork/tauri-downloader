import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";

test("the compiled video overlay does not restyle the visited page", async ({ page }) => {
  await page.setContent(
    `<style>button {background:blue; color:white; border:3px solid red; --tw-shadow:7px 7px red} body {font:20px serif}</style><button id="host">Original</button><button class="fluxor-capture-button">Descargar con Fluxor</button>`,
  );
  const host = page.locator("#host");
  const before = await host.evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      background: style.backgroundColor,
      border: style.border,
      font: style.font,
      shadow: style.getPropertyValue("--tw-shadow"),
    };
  });
  await page.addStyleTag({
    content: await readFile("browser-extension/generated/overlay.css", "utf8"),
  });
  expect(
    await host.evaluate((element) => {
      const style = getComputedStyle(element);
      return {
        background: style.backgroundColor,
        border: style.border,
        font: style.font,
        shadow: style.getPropertyValue("--tw-shadow"),
      };
    }),
  ).toEqual(before);
  const overlay = page.locator(".fluxor-capture-button");
  const normal = await overlay.evaluate((element) => getComputedStyle(element).backgroundColor);
  expect(normal).not.toBe(before.background);
  await expect(overlay).toHaveCSS("border-top-width", "0px");
  await overlay.evaluate((element) => {
    (element as HTMLElement).dataset.state = "error";
  });
  expect(await overlay.evaluate((element) => getComputedStyle(element).backgroundColor)).not.toBe(
    normal,
  );
  expect(
    await page.evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue("--fluxor-color-accent"),
    ),
  ).toBe("");
});
