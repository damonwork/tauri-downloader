import { readFile, writeFile } from "node:fs/promises";

// Content-script CSS shares the document with the visited site. Scope Tailwind's
// generated fallback defaults and internal properties as well as the theme.
// The popup uses its own document and does not need this isolation step.
const output = new URL("../browser-extension/generated/overlay.css", import.meta.url);
const css = (await readFile(output, "utf8"))
  .replace(/--tw-/g, "--fluxor-tw-")
  .replace(/:root,\s*:host/g, ".fluxor-capture-button")
  .replace(
    /\*,\s*:before,\s*:after,\s*::backdrop/g,
    ".fluxor-capture-button,.fluxor-capture-button:before,.fluxor-capture-button:after",
  );
if (css.includes(":root") || css.includes(":host") || css.includes("--tw-")) {
  throw new Error(
    "Overlay CSS escaped its namespace; inspect the Tailwind output before packaging.",
  );
}
await writeFile(output, css);
