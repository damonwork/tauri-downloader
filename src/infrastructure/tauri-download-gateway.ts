import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  DownloadGateway,
  BrowserIntegration,
  ProgressListener,
  RuntimeCapabilities,
  SnapshotListener,
  Unlisten,
} from "@/application/download-gateway";
import type {
  CreateDownloadInput,
  DownloadAction,
  DownloadItem,
  DownloadProgressEvent,
  DownloadSource,
} from "@/domain/download";
import type { AppSettings, AppSnapshot, ProxyProfile } from "@/domain/settings";

export class TauriDownloadGateway implements DownloadGateway {
  readonly capabilities: RuntimeCapabilities = {
    runtime: "tauri",
    canRevealDownloads: true,
  };

  snapshot(): Promise<AppSnapshot> {
    return invoke("get_snapshot");
  }

  browserIntegration(): Promise<BrowserIntegration> {
    return invoke("get_browser_integration");
  }

  revealDownload(id: string): Promise<void> {
    return invoke("reveal_download", { id });
  }

  async subscribe(listener: SnapshotListener, progressListener: ProgressListener): Promise<Unlisten> {
    let active = true;
    let refreshing = false;
    let pending = false;
    const refresh = async (): Promise<void> => {
      if (refreshing) {
        pending = true;
        return;
      }
      refreshing = true;
      try {
        do {
          pending = false;
          const snapshot = await this.snapshot();
          if (active) listener(snapshot);
        } while (active && pending);
      } catch {
        if (active) {
          pending = true;
          window.setTimeout(() => void refresh(), 500);
        }
      } finally {
        refreshing = false;
      }
    };
    const [stopChanged, stopProgress] = await Promise.all([
      listen("downloads://changed", () => {
        void refresh();
      }),
      listen<DownloadProgressEvent>("downloads://progress", ({ payload }) => {
        if (active) progressListener(payload);
      }),
    ]);
    return () => {
      active = false;
      stopChanged();
      stopProgress();
    };
  }

  add(input: CreateDownloadInput): Promise<DownloadItem> {
    return invoke("add_download", { input });
  }

  control(id: string, action: DownloadAction): Promise<void> {
    return invoke("control_download", { id, action });
  }

  replaceSource(id: string, source: DownloadSource): Promise<void> {
    return invoke("replace_download_source", { id, source });
  }

  updateSettings(settings: AppSettings): Promise<void> {
    return invoke("update_settings", { settings });
  }

  saveProxy(proxy: ProxyProfile): Promise<void> {
    return invoke("save_proxy", { proxy });
  }

  removeProxy(id: string): Promise<void> {
    return invoke("remove_proxy", { id });
  }

  checkProxy(id: string): Promise<void> {
    return invoke("check_proxy", { id });
  }
}
