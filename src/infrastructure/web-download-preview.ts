import type { DownloadItem, DownloadSource, SegmentState, TransferTelemetry } from "@/domain/download";
import { EMPTY_TRANSFER_TELEMETRY, segmentSize } from "@/domain/download";
import { createId } from "@/domain/id";
import { DEFAULT_SETTINGS, type AppSnapshot, type ProxyProfile } from "@/domain/settings";

const MIN_SEGMENT_SIZE = 2 * 1024 * 1024;

export function demoSnapshot(): AppSnapshot {
  const now = Date.now();
  const source = (url: string): DownloadSource => ({ url, headers: [], cookies: [], proxy: { kind: "direct" } });
  const activeTelemetry = demoSegmentedTelemetry(3_842_000_000, [0.86, 0.72, 0, 0.66, 0.59, 0.48, 1, 0.31], 18_700_000);
  activeTelemetry.segments[2].state = "connecting";
  activeTelemetry.segments[2].speedBytes = 0;
  const pausedTelemetry = demoSegmentedTelemetry(1_460_000_000, [0.5, 0.48, 0.47, 0.46, 0.45, 0.44, 0.43, 0.42], 0);
  pausedTelemetry.phase = "idle";
  pausedTelemetry.segments.forEach((segment) => {
    if (segment.state !== "completed") segment.state = "paused";
  });
  return {
    revision: 1,
    settings: { ...DEFAULT_SETTINGS },
    proxies: [
      {
        id: createId(),
        name: "Salida principal",
        url: "socks5://127.0.0.1:1080",
        enabled: false,
        health: { kind: "untested" },
      },
    ],
    downloads: [
      {
        id: createId(), fileName: "DaVinci_Resolve_19_Linux.zip", category: "archive", state: { kind: "downloading", speedBytes: 18_700_000 },
        source: source("https://downloads.example.com/DaVinci_Resolve_19_Linux.zip"), destination: "Fluxor/Comprimidos",
        transfer: { downloadedBytes: telemetryDownloaded(activeTelemetry), size: { kind: "known", totalBytes: 3_842_000_000 }, validator: { kind: "etag", value: "demo-etag" }, resume: { kind: "supported" } }, telemetry: activeTelemetry, threads: 8, speedLimitBytes: 0,
        createdAt: new Date(now - 2_820_000).toISOString(), updatedAt: new Date().toISOString(),
      },
      {
        id: createId(), fileName: "curso-arquitectura-modular.mkv", category: "video", state: { kind: "paused" },
        source: source("https://media.example.net/curso-arquitectura-modular.mkv"), destination: "Fluxor/Videos",
        transfer: { downloadedBytes: telemetryDownloaded(pausedTelemetry), size: { kind: "known", totalBytes: 1_460_000_000 }, validator: { kind: "etag", value: "demo-paused-etag" }, resume: { kind: "supported" } }, telemetry: pausedTelemetry, threads: 8, speedLimitBytes: 5 * 1024 ** 2,
        createdAt: new Date(now - 7_200_000).toISOString(), updatedAt: new Date(now - 900_000).toISOString(),
      },
      {
        id: createId(), fileName: "dataset-2026-08.tar.gz", category: "archive", state: { kind: "queued" },
        source: source("https://data.example.org/dataset-2026-08.tar.gz"), destination: "Fluxor/Comprimidos",
        transfer: { downloadedBytes: 0, size: { kind: "known", totalBytes: 824_000_000 }, validator: { kind: "none" }, resume: { kind: "unknown" } }, telemetry: emptyTelemetry(), threads: 6, speedLimitBytes: 0,
        createdAt: new Date(now - 420_000).toISOString(), updatedAt: new Date(now - 420_000).toISOString(),
      },
      {
        id: createId(), fileName: "manual-servicio.pdf", category: "document", state: { kind: "completed", completedAt: new Date(now - 82_800_000).toISOString() },
        source: source("https://docs.example.com/manual-servicio.pdf"), destination: "Fluxor/Documentos",
        transfer: { downloadedBytes: 28_400_000, size: { kind: "known", totalBytes: 28_400_000 }, validator: { kind: "none" }, resume: { kind: "unsupported", reason: "El servidor no acepta solicitudes por rango" } }, telemetry: demoSingleTelemetry(28_400_000, 28_400_000, "completed"), threads: 4, speedLimitBytes: 0,
        createdAt: new Date(now - 86_400_000).toISOString(), updatedAt: new Date(now - 82_800_000).toISOString(),
      },
    ],
  };
}

export function previewSource(source: DownloadSource): DownloadSource {
  let url = "https://invalid.local/";
  try {
    const parsed = new URL(source.url);
    parsed.search = "";
    parsed.hash = "";
    url = parsed.toString();
  } catch {
    // Invalid legacy entries are retained only as redacted placeholders.
  }
  return {
    url,
    headers: [],
    cookies: [],
    proxy: structuredClone(source.proxy),
  };
}

export function previewProxy(proxy: ProxyProfile): ProxyProfile {
  try {
    const url = new URL(proxy.url);
    url.username = "";
    url.password = "";
    return { ...structuredClone(proxy), url: url.toString() };
  } catch {
    return { ...structuredClone(proxy), url: "invalid-proxy://redacted" };
  }
}

export function emptyTelemetry(): TransferTelemetry {
  return structuredClone(EMPTY_TRANSFER_TELEMETRY);
}

export function normalizeTelemetry(telemetry: TransferTelemetry | undefined): TransferTelemetry {
  if (!telemetry) return emptyTelemetry();
  return {
    phase: telemetry.phase ?? "idle",
    mode: telemetry.mode ?? { kind: "pending" },
    segments: telemetry.segments ?? [],
  };
}

export function legacyTelemetry(item: DownloadItem): TransferTelemetry {
  const totalBytes = item.transfer.size.kind === "known" ? item.transfer.size.totalBytes : undefined;
  const state: SegmentState = item.state.kind === "completed"
    ? "completed"
    : item.state.kind === "failed"
      ? "failed"
      : item.state.kind === "paused"
        ? "paused"
        : "pending";
  return {
    phase: "idle",
    mode: { kind: "single", reason: "Progreso restaurado sin detalle histórico por segmento" },
    segments: [{
      index: 0,
      startByte: 0,
      endByte: totalBytes && totalBytes > 0 ? totalBytes - 1 : null,
      downloadedBytes: totalBytes
        ? Math.min(totalBytes, item.transfer.downloadedBytes)
        : item.transfer.downloadedBytes,
      speedBytes: 0,
      state,
      lastActivityAt: item.updatedAt,
      error: item.state.kind === "failed" ? item.state.message : null,
    }],
  };
}

export function initializePreviewTelemetry(item: DownloadItem): void {
  if (item.transfer.size.kind !== "known" || item.telemetry.segments.length > 0) return;
  const usefulSegments = Math.max(1, Math.ceil(item.transfer.size.totalBytes / MIN_SEGMENT_SIZE));
  const segmentCount = Math.min(item.threads, usefulSegments);
  if (segmentCount > 1) {
    item.transfer.validator = { kind: "etag", value: `preview-${item.id}` };
    item.transfer.resume = { kind: "supported" };
    item.telemetry = demoSegmentedTelemetry(
      item.transfer.size.totalBytes,
      Array.from({ length: segmentCount }, () => 0),
      item.state.kind === "downloading" ? item.state.speedBytes : 0,
    );
  } else {
    item.telemetry = demoSingleTelemetry(
      item.transfer.size.totalBytes,
      0,
      "connecting",
      "Configurado para un único flujo",
    );
  }
}

export function advancePreviewDownload(item: DownloadItem): boolean {
  if (item.state.kind !== "downloading" || item.transfer.size.kind !== "known") return false;
  const sampledAt = new Date().toISOString();
  item.telemetry.phase = "transferring";
  for (const segment of item.telemetry.segments) {
    const total = segmentSize(segment);
    if (!total || segment.state === "completed") continue;
    if (segment.state === "connecting" && Math.random() < 0.45) {
      segment.speedBytes = 0;
      continue;
    }
    segment.state = "downloading";
    const baseline = Math.max(320_000, item.state.speedBytes / Math.max(1, item.telemetry.segments.length));
    segment.speedBytes = Math.round(baseline * (0.7 + Math.random() * 0.6));
    segment.downloadedBytes = Math.min(total, segment.downloadedBytes + segment.speedBytes * 0.9);
    segment.lastActivityAt = sampledAt;
    if (segment.downloadedBytes >= total) {
      segment.downloadedBytes = total;
      segment.speedBytes = 0;
      segment.state = "completed";
    }
  }
  item.state.speedBytes = item.telemetry.segments.reduce((sum, segment) => sum + segment.speedBytes, 0);
  item.transfer.downloadedBytes = telemetryDownloaded(item.telemetry);
  item.updatedAt = sampledAt;
  if (item.transfer.downloadedBytes >= item.transfer.size.totalBytes) {
    item.state = { kind: "completed", completedAt: new Date().toISOString() };
    item.telemetry.phase = "finalizing";
  }
  return true;
}

function demoSegmentedTelemetry(totalBytes: number, ratios: number[], aggregateSpeed: number): TransferTelemetry {
  const segmentBytes = Math.ceil(totalBytes / Math.max(1, ratios.length));
  const active = Math.max(1, ratios.filter((ratio) => ratio > 0 && ratio < 1).length);
  const sampledAt = new Date().toISOString();
  return {
    phase: "transferring",
    mode: { kind: "segmented" },
    segments: ratios.map((ratio, index) => {
      const startByte = index * segmentBytes;
      const endByte = Math.min(totalBytes, (index + 1) * segmentBytes) - 1;
      const total = Math.max(0, endByte - startByte + 1);
      const downloadedBytes = Math.round(total * Math.min(1, Math.max(0, ratio)));
      return {
        index,
        startByte,
        endByte,
        downloadedBytes,
        speedBytes: ratio > 0 && ratio < 1 ? Math.round(aggregateSpeed / active) : 0,
        state: ratio >= 1 ? "completed" : ratio > 0 ? "downloading" : "pending",
        lastActivityAt: ratio > 0 ? sampledAt : null,
        error: null,
      };
    }),
  };
}

function demoSingleTelemetry(
  totalBytes: number,
  downloadedBytes: number,
  state: SegmentState,
  reason = "El servidor no admite una segmentación segura",
): TransferTelemetry {
  return {
    phase: state === "completed" ? "finalizing" : "transferring",
    mode: { kind: "single", reason },
    segments: [{
      index: 0,
      startByte: 0,
      endByte: totalBytes > 0 ? totalBytes - 1 : null,
      downloadedBytes,
      speedBytes: 0,
      state,
      lastActivityAt: new Date().toISOString(),
      error: null,
    }],
  };
}

function telemetryDownloaded(telemetry: TransferTelemetry): number {
  return telemetry.segments.reduce((sum, segment) => sum + segment.downloadedBytes, 0);
}
