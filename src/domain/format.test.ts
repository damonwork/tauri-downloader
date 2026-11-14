import { describe, expect, it } from "vitest";
import { formatBytes, formatSpeed, formatEta } from "./format";

describe("transfer values", () => {
  it("formats binary byte rates consistently with speed limits", () => {
    expect(formatSpeed(1024)).toBe("1 KB/s");
    expect(formatBytes(1024 ** 2)).toBe("1.0 MB");
    expect(formatBytes(0.5)).toBe("1 B");
  });
  it("does not render invalid rates or sizes as NaN or Infinity", () => {
    for (const value of [NaN, Infinity, -1]) {
      expect(formatBytes(value)).toBe("—");
      expect(formatSpeed(value)).toBe("—");
      expect(formatEta(100, 0, value)).toBe("—");
    }
  });
  it("carries rounded minutes into hours", () => {
    expect(formatEta(7199, 0, 1)).toBe("2h 0m");
    expect(formatEta(3599, 0, 1)).toBe("1h 0m");
    expect(formatEta(61, 0, 1)).toBe("2m");
    expect(formatEta(10, 0, 1)).toBe("10s");
  });
  it("withholds ETA when unknown, stalled, paused or fully received", () => {
    expect(formatEta(undefined, 10, 1)).toBe("—");
    expect(formatEta(100, 20, 0)).toBe("—");
    expect(formatEta(100, 100, 20)).toBe("—");
    expect(formatEta(Infinity, 10, 1)).toBe("—");
  });
});
