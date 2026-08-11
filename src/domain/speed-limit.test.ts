import { describe, expect, it } from "vitest";
import {
  MAX_SPEED_LIMIT_BYTES,
  maxAmountForUnit,
  speedLimitBytes,
  speedLimitInput,
  type SpeedUnit,
} from "./speed-limit";

describe("speed limits", () => {
  it.each<[number, SpeedUnit, number]>([
    [512, "KB", 512 * 1024],
    [2.5, "MB", 2.5 * 1024 ** 2],
    [1, "GB", 1024 ** 3],
  ])("convierte %s %s a bytes por segundo", (amount, unit, expected) => {
    expect(speedLimitBytes(amount, unit)).toBe(expected);
  });

  it("usa cero como valor explícito para no limitar", () => {
    expect(speedLimitBytes(0, "GB")).toBe(0);
    expect(speedLimitBytes(Number.NaN, "MB")).toBe(0);
    expect(speedLimitInput(0)).toEqual({ amount: 0, unit: "MB" });
  });

  it("limita la entrada al máximo aceptado por Rust", () => {
    expect(speedLimitBytes(11, "GB")).toBe(MAX_SPEED_LIMIT_BYTES);
    expect(maxAmountForUnit("GB")).toBe(10);
  });

  it("elige una unidad legible y conserva unidades solicitadas", () => {
    expect(speedLimitInput(5 * 1024 ** 2)).toEqual({ amount: 5, unit: "MB" });
    expect(speedLimitInput(512 * 1024)).toEqual({ amount: 512, unit: "KB" });
    expect(speedLimitInput(1024 ** 3, "MB")).toEqual({ amount: 1024, unit: "MB" });
  });
});
