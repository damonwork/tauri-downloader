import { describe, expect, it } from "vitest";
import {
  categoryForFile,
  applyDownloadProgress,
  deriveStats,
  EMPTY_TRANSFER_TELEMETRY,
  fileNameFromUrl,
  progressOf,
  safeFileName,
  segmentProgressOf,
  type DownloadItem,
} from "./download";
import { formatSpeed, redactUrl } from "./format";

const baseItem: DownloadItem = {
  id: "one",
  fileName: "file.zip",
  category: "archive",
  state: { kind: "queued" },
  source: { url: "https://example.com/file.zip", headers: [], cookies: [], proxy: { kind: "direct" }, forceSingleStream: false },
  destination: "Fluxor",
  transfer: { downloadedBytes: 25, size: { kind: "known", totalBytes: 100 }, validator: { kind: "none" }, resume: { kind: "unknown" } },
  telemetry: structuredClone(EMPTY_TRANSFER_TELEMETRY),
  threads: 8,
  speedLimitBytes: 0,
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

  it("calcula cada segmento desde su propio rango", () => {
    expect(segmentProgressOf({
      index: 1,
      startByte: 100,
      endByte: 199,
      downloadedBytes: 42,
      speedBytes: 10,
      state: "downloading",
      lastActivityAt: null,
      error: null,
    })).toBe(42);
  });

  it("aplica una muestra incremental sin reemplazar otras descargas", () => {
    const other = { ...baseItem, id: "other" };
    const telemetry = {
      ...structuredClone(EMPTY_TRANSFER_TELEMETRY),
      phase: "connecting" as const,
    };
    const items = applyDownloadProgress([baseItem, other], {
      revision: 3,
      downloadId: baseItem.id,
      state: { kind: "downloading", speedBytes: 2_048 },
      transfer: { ...baseItem.transfer, downloadedBytes: 40 },
      telemetry,
      updatedAt: "2026-08-11T00:01:00Z",
    });

    expect(items[0].telemetry.phase).toBe("connecting");
    expect(items[0].transfer.downloadedBytes).toBe(40);
    expect(items[1]).toBe(other);
  });

  it("clasifica y sanea nombres de archivo", () => {
    expect(categoryForFile("movie.MKV")).toBe("video");
    expect(categoryForFile("backup.tar.gz")).toBe("archive");
    expect(safeFileName("report: final?.pdf ")).toBe("report_ final_.pdf");
    expect(safeFileName(`${"a".repeat(250)}.zip`)).toHaveLength(200);
  });

  it("deriva nombres desde rutas y metadatos de URLs", () => {
    expect(fileNameFromUrl("https://example.com/files/report%20final.pdf?token=secret")).toBe("report final.pdf");
    expect(fileNameFromUrl("https://example.com/download?filename=video.mp4")).toBe("video.mp4");
    expect(fileNameFromUrl("https://example.com/download?response-content-disposition=attachment%3B%20filename%3Dmanual.pdf")).toBe("manual.pdf");
  });

  it("oculta credenciales y query strings en URLs mostradas", () => {
    expect(redactUrl("https://user:secret@example.com/file.zip?token=abc#part")).toBe(
      "https://example.com/file.zip?•••",
    );
  });

  it("muestra la velocidad en la unidad más cercana", () => {
    expect(formatSpeed(504 * 1024)).toBe("504 KB/s");
    expect(formatSpeed(2 * 1024 ** 2)).toBe("2 MB/s");
    expect(formatSpeed(2.4 * 1024 ** 2)).toBe("2.4 MB/s");
  });
});
