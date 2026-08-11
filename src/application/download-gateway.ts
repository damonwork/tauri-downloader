import type {
  CreateDownloadInput,
  DownloadAction,
  DownloadItem,
  DownloadProgressEvent,
  DownloadSource,
} from "@/domain/download";
import type { AppSettings, AppSnapshot, ProxyProfile } from "@/domain/settings";

export interface RuntimeCapabilities {
  runtime: "web" | "tauri";
}

export type SnapshotListener = (snapshot: AppSnapshot) => void;
export type ProgressListener = (progress: DownloadProgressEvent) => void;
export type Unlisten = () => void;

export interface DownloadGateway {
  readonly capabilities: RuntimeCapabilities;
  snapshot(): Promise<AppSnapshot>;
  subscribe(listener: SnapshotListener, progressListener: ProgressListener): Promise<Unlisten>;
  add(input: CreateDownloadInput): Promise<DownloadItem>;
  control(id: string, action: DownloadAction): Promise<void>;
  replaceSource(id: string, source: DownloadSource): Promise<void>;
  updateSettings(settings: AppSettings): Promise<void>;
  saveProxy(proxy: ProxyProfile): Promise<void>;
  removeProxy(id: string): Promise<void>;
  checkProxy(id: string): Promise<void>;
}
