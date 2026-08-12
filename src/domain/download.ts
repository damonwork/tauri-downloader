export type DownloadCategory = "video" | "archive" | "document" | "audio" | "other";

export interface HeaderEntry {
  name: string;
  value: string;
}

export interface CookieEntry {
  name: string;
  value: string;
}

export interface DownloadSource {
  url: string;
  headers: HeaderEntry[];
  cookies: CookieEntry[];
  proxy: { kind: "direct" } | { kind: "profile"; profileId: string };
  forceSingleStream: boolean;
}

export type TransferSize = { kind: "unknown" } | { kind: "known"; totalBytes: number };

export type SourceValidator =
  | { kind: "none" }
  | { kind: "etag"; value: string }
  | { kind: "lastModified"; value: string };

export type ResumeSupport =
  | { kind: "unknown" }
  | { kind: "supported" }
  | { kind: "unsupported"; reason: string };

export interface TransferProgress {
  downloadedBytes: number;
  size: TransferSize;
  validator: SourceValidator;
  resume: ResumeSupport;
}

export type TransferPhase =
  | "idle"
  | "preparing"
  | "probing"
  | "connecting"
  | "transferring"
  | "merging"
  | "finalizing";

export type TransferMode =
  | { kind: "pending" }
  | { kind: "single"; reason: string | null }
  | { kind: "segmented" };

export type SegmentState =
  | "pending"
  | "connecting"
  | "downloading"
  | "paused"
  | "completed"
  | "failed"
  | "stopped";

export interface SegmentProgress {
  index: number;
  startByte: number;
  endByte: number | null;
  downloadedBytes: number;
  speedBytes: number;
  state: SegmentState;
  lastActivityAt: string | null;
  error: string | null;
}

export interface TransferTelemetry {
  phase: TransferPhase;
  mode: TransferMode;
  segments: SegmentProgress[];
}

export const EMPTY_TRANSFER_TELEMETRY: TransferTelemetry = {
  phase: "idle",
  mode: { kind: "pending" },
  segments: [],
};

export type DownloadState =
  | { kind: "queued" }
  | { kind: "downloading"; speedBytes: number }
  | { kind: "paused" }
  | { kind: "completed"; completedAt: string }
  | { kind: "failed"; message: string; recoverable: boolean };

export interface DownloadItem {
  id: string;
  fileName: string;
  category: DownloadCategory;
  state: DownloadState;
  source: DownloadSource;
  destination: string;
  transfer: TransferProgress;
  telemetry: TransferTelemetry;
  threads: number;
  speedLimitBytes: number;
  createdAt: string;
  updatedAt: string;
}

export type DownloadAction = "pause" | "resume" | "retry" | "restart" | "remove";

export interface DownloadProgressEvent {
  revision: number;
  downloadId: string;
  state: DownloadState;
  transfer: TransferProgress;
  telemetry: TransferTelemetry;
  updatedAt: string;
}

export interface CreateDownloadInput {
  source: DownloadSource;
  fileName: string;
  fileNameCustomized: boolean;
  category: DownloadCategory;
  categoryCustomized: boolean;
  destination: string;
  destinationCustomized: boolean;
  threads: number;
  speedLimitBytes: number;
  startImmediately: boolean;
}

export interface ParsedRequest {
  source: DownloadSource;
  fileName: string;
  warnings: string[];
}

export interface DownloadStats {
  active: number;
  queued: number;
  completed: number;
  failed: number;
  speedBytes: number;
}

export function progressOf(item: DownloadItem): number {
  if (item.transfer.size.kind === "unknown" || item.transfer.size.totalBytes <= 0) return 0;
  return Math.min(100, Math.round((item.transfer.downloadedBytes / item.transfer.size.totalBytes) * 100));
}

export function segmentSize(segment: SegmentProgress): number | undefined {
  if (segment.endByte === null || segment.endByte < segment.startByte) return undefined;
  return segment.endByte - segment.startByte + 1;
}

export function segmentProgressOf(segment: SegmentProgress): number | undefined {
  const total = segmentSize(segment);
  if (!total) return undefined;
  return Math.min(100, Math.round((segment.downloadedBytes / total) * 100));
}

export function applyDownloadProgress(items: DownloadItem[], progress: DownloadProgressEvent): DownloadItem[] {
  return items.map((item) => item.id === progress.downloadId
    ? {
        ...item,
        state: progress.state,
        transfer: progress.transfer,
        telemetry: progress.telemetry,
        updatedAt: progress.updatedAt,
      }
    : item);
}

export function deriveStats(items: DownloadItem[]): DownloadStats {
  return items.reduce<DownloadStats>(
    (stats, item) => {
      if (item.state.kind === "downloading") {
        stats.active += 1;
        stats.speedBytes += item.state.speedBytes;
      }
      if (item.state.kind === "queued") stats.queued += 1;
      if (item.state.kind === "completed") stats.completed += 1;
      if (item.state.kind === "failed") stats.failed += 1;
      return stats;
    },
    { active: 0, queued: 0, completed: 0, failed: 0, speedBytes: 0 },
  );
}

export function categoryForFile(fileName: string): DownloadCategory {
  const extension = fileName.split(".").pop()?.toLowerCase() ?? "";
  if (["mp4", "mkv", "mov", "webm", "avi"].includes(extension)) return "video";
  if (["zip", "rar", "7z", "tar", "gz"].includes(extension)) return "archive";
  if (["pdf", "doc", "docx", "xls", "xlsx", "txt"].includes(extension)) return "document";
  if (["mp3", "wav", "flac", "m4a", "ogg"].includes(extension)) return "audio";
  return "other";
}

export function safeFileName(value: string): string {
  const clean = value
    .replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_")
    .replace(/\s+/g, " ")
    .replace(/[. ]+$/g, "")
    .trim();
  return truncateFileName(clean || "descarga", 200);
}

export function fileNameFromUrl(value: string): string {
  const url = new URL(value);
  const disposition = url.searchParams.get("response-content-disposition");
  const queryName = url.searchParams.get("filename") ?? url.searchParams.get("file_name");
  const finalSegment = decodeUrlComponent(url.pathname.split("/").filter(Boolean).pop() ?? "");
  return safeFileName(fileNameFromDisposition(disposition) || queryName || finalSegment || `descarga-${url.hostname}`);
}

function fileNameFromDisposition(value: string | null): string {
  if (!value) return "";
  const extended = value.match(/filename\*\s*=\s*UTF-8''([^;]+)/i)?.[1];
  if (extended) return decodeUrlComponent(extended.trim().replace(/^"|"$/g, ""));
  return value.match(/filename\s*=\s*"([^"]+)"/i)?.[1]
    ?? value.match(/filename\s*=\s*([^;]+)/i)?.[1]?.trim()
    ?? "";
}

function decodeUrlComponent(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function truncateFileName(value: string, maxUnits: number): string {
  if (value.length <= maxUnits) return value;
  const extensionAt = value.lastIndexOf(".");
  const extension = extensionAt > 0 && value.length - extensionAt <= 20 ? value.slice(extensionAt) : "";
  const budget = maxUnits - extension.length;
  let stem = "";
  for (const character of value.slice(0, extension ? extensionAt : value.length)) {
    if (stem.length + character.length > budget) break;
    stem += character;
  }
  return `${stem.replace(/[. ]+$/g, "")}${extension}` || "descarga";
}
