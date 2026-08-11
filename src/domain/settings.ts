export interface ProxyProfile {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
  health:
    | { kind: "untested" }
    | { kind: "checking" }
    | { kind: "online"; latencyMs: number }
    | { kind: "offline"; reason: string };
}

export interface CategoryDirectories {
  video: string;
  archive: string;
  document: string;
  audio: string;
  other: string;
}

export interface AppSettings {
  maxConcurrent: number;
  defaultThreads: number;
  downloadDirectory: string;
  organizeByCategory: boolean;
  categoryDirectories: CategoryDirectories;
  startImmediately: boolean;
}

export interface AppSnapshot {
  revision: number;
  downloads: DownloadItem[];
  proxies: ProxyProfile[];
  settings: AppSettings;
}

export const DEFAULT_SETTINGS: AppSettings = {
  maxConcurrent: 3,
  defaultThreads: 8,
  downloadDirectory: "Fluxor",
  organizeByCategory: true,
  categoryDirectories: {
    video: "Videos",
    archive: "Comprimidos",
    document: "Documentos",
    audio: "Audio",
    other: "Otros",
  },
  startImmediately: true,
};

export function destinationForCategory(settings: AppSettings, category: DownloadCategory): string {
  if (!settings.organizeByCategory) return settings.downloadDirectory;
  const root = settings.downloadDirectory.trim().replace(/[/\\]+$/g, "");
  const categoryDirectory = settings.categoryDirectories[category]
    .trim()
    .replace(/^[/\\]+|[/\\]+$/g, "");
  return categoryDirectory ? `${root}/${categoryDirectory}` : root;
}
import type { DownloadCategory, DownloadItem } from "./download";
