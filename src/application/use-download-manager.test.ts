import { describe, expect, it } from "vitest";
import { overlayDownloadProgress } from "./use-download-manager";
import { EMPTY_TRANSFER_TELEMETRY, type DownloadItem, type DownloadProgressEvent } from "@/domain/download";
import { DEFAULT_SETTINGS, type AppSnapshot } from "@/domain/settings";

function item(id: string): DownloadItem {
  return {
    id,
    fileName: `${id}.zip`,
    category: "archive",
    state: { kind: "downloading", speedBytes: 10 },
    source: { url: `https://example.com/${id}.zip`, headers: [], cookies: [], proxy: { kind: "direct" } },
    destination: "Fluxor",
    transfer: { downloadedBytes: 10, size: { kind: "known", totalBytes: 100 }, validator: { kind: "none" }, resume: { kind: "unknown" } },
    telemetry: structuredClone(EMPTY_TRANSFER_TELEMETRY),
    threads: 4,
    speedLimitBytes: 0,
    createdAt: "2026-08-11T00:00:00Z",
    updatedAt: "2026-08-11T00:00:01Z",
  };
}

describe("download progress ordering", () => {
  it("preserva un estado terminal de A al superponer un delta posterior de B", () => {
    const completed = {
      ...item("a"),
      state: { kind: "completed" as const, completedAt: "2026-08-11T00:01:00Z" },
    };
    const snapshot: AppSnapshot = {
      revision: 101,
      downloads: [completed, item("b")],
      proxies: [],
      settings: structuredClone(DEFAULT_SETTINGS),
    };
    const progress: DownloadProgressEvent = {
      revision: 102,
      downloadId: "b",
      state: { kind: "downloading", speedBytes: 20 },
      transfer: { ...item("b").transfer, downloadedBytes: 30 },
      telemetry: structuredClone(EMPTY_TRANSFER_TELEMETRY),
      updatedAt: "2026-08-11T00:01:01Z",
    };

    const merged = overlayDownloadProgress(snapshot, [progress]);

    expect(merged.revision).toBe(102);
    expect(merged.downloads[0].state.kind).toBe("completed");
    expect(merged.downloads[1].transfer.downloadedBytes).toBe(30);
  });
});
