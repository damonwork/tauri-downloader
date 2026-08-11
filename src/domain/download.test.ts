import { describe, expect, it } from "vitest";
import { categoryForFile, deriveStats, progressOf, safeFileName, type DownloadItem } from "./download";
import { redactUrl } from "./format";

const baseItem: DownloadItem = {
  id: "one",
  fileName: "file.zip",
  category: "archive",
  state: { kind: "queued" },
  source: { url: "https://example.com/file.zip", headers: [], cookies: [], proxy: { kind: "direct" } },
  destination: "Fluxor",
  transfer: { downloadedBytes: 25, size: { kind: "known", totalBytes: 100 }, validator: { kind: "none" } },
  threads: 8,
  createdAt: "2026-08-11T00:00:00Z",
  updatedAt: "2026-08-11T00:00:00Z",
};

describe("download domain", () => {
  it("calcula progreso solo cuando el tamaño es conocido", () => {
    expect(progressOf(baseItem)).toBe(25);
    expect(progressOf({ ...baseItem, transfer: { ...baseItem.transfer, size: { kind: "unknown" } } })).toBe(0);
  });

  it("deriva métricas a partir de estados discriminados", () => {
    const items: DownloadItem[] = [
      { ...baseItem, id: "active", state: { kind: "downloading", speedBytes: 2_000 } },
      { ...baseItem, id: "queued" },
      { ...baseItem, id: "done", state: { kind: "completed", completedAt: "2026-08-11T01:00:00Z" } },
      { ...baseItem, id: "failed", state: { kind: "failed", message: "timeout", recoverable: true } },
    ];

    expect(deriveStats(items)).toEqual({ active: 1, queued: 1, completed: 1, failed: 1, speedBytes: 2_000 });
  });

  it("clasifica y sanea nombres de archivo", () => {
    expect(categoryForFile("movie.MKV")).toBe("video");
    expect(categoryForFile("backup.tar.gz")).toBe("archive");
    expect(safeFileName("report: final?.pdf ")).toBe("report_ final_.pdf");
  });

  it("oculta credenciales y query strings en URLs mostradas", () => {
    expect(redactUrl("https://user:secret@example.com/file.zip?token=abc#part")).toBe(
      "https://example.com/file.zip?•••",
    );
  });
});
