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
}

export type TransferSize = { kind: "unknown" } | { kind: "known"; totalBytes: number };

export type SourceValidator =
  | { kind: "none" }
  | { kind: "etag"; value: string }
  | { kind: "lastModified"; value: string };

export interface TransferProgress {
  downloadedBytes: number;
  size: TransferSize;
  validator: SourceValidator;
}

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
  threads: number;
  createdAt: string;
  updatedAt: string;
}

export type DownloadAction = "pause" | "resume" | "retry" | "restart" | "remove";

export interface CreateDownloadInput {
  source: DownloadSource;
  fileName: string;
  category: DownloadCategory;
  destination: string;
  threads: number;
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
  return clean || "descarga";
}

export function fileNameFromUrl(value: string): string {
  const url = new URL(value);
  const finalSegment = decodeURIComponent(url.pathname.split("/").filter(Boolean).pop() ?? "");
  return safeFileName(finalSegment || `${url.hostname}-${Date.now()}`);
}
