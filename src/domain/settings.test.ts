import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS, destinationForCategory } from "./settings";

describe("download destinations", () => {
  it("organiza rutas relativas dentro de la raíz configurada", () => {
    expect(destinationForCategory(DEFAULT_SETTINGS, "video")).toBe("Fluxor/Videos");
    expect(destinationForCategory(DEFAULT_SETTINGS, "archive")).toBe("Fluxor/Comprimidos");
  });

  it("permite desactivar las subcarpetas por categoría", () => {
    expect(destinationForCategory({ ...DEFAULT_SETTINGS, organizeByCategory: false }, "audio")).toBe("Fluxor");
  });

  it("conserva una raíz absoluta configurable", () => {
    const settings = { ...DEFAULT_SETTINGS, downloadDirectory: "D:\\Transferencias" };
    expect(destinationForCategory(settings, "document")).toBe("D:\\Transferencias/Documentos");
    expect(destinationForCategory({ ...settings, downloadDirectory: "/srv/downloads" }, "audio"))
      .toBe("/srv/downloads/Audio");
  });
});
