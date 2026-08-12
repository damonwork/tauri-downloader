import type {
  DownloadGateway,
  ProgressListener,
  RuntimeCapabilities,
  SnapshotListener,
  Unlisten,
} from "@/application/download-gateway";
import type { CreateDownloadInput, DownloadAction, DownloadItem, DownloadSource } from "@/domain/download";
import { categoryForFile } from "@/domain/download";
import { createId } from "@/domain/id";
import { DEFAULT_SETTINGS, type AppSettings, type AppSnapshot, type ProxyProfile } from "@/domain/settings";
import {
  advancePreviewDownload,
  demoSnapshot,
  emptyTelemetry,
  initializePreviewTelemetry,
  legacyTelemetry,
  normalizeTelemetry,
  previewProxy,
  previewSource,
} from "@/infrastructure/web-download-preview";

const STORAGE_KEY = "fluxor.preview.snapshot.v2";
const LEGACY_STORAGE_KEY = "fluxor.preview.snapshot.v1";

export class WebDownloadGateway implements DownloadGateway {
  readonly capabilities: RuntimeCapabilities = {
    runtime: "web",
  };

  private state: AppSnapshot;
  private readonly listeners = new Set<SnapshotListener>();
  private readonly timer: number;

  constructor() {
    this.state = this.load();
    this.schedule();
    localStorage.setItem(STORAGE_KEY, JSON.stringify(this.state));
    this.timer = window.setInterval(() => this.tick(), 900);
  }

  async snapshot(): Promise<AppSnapshot> {
    return structuredClone(this.state);
  }

  async subscribe(listener: SnapshotListener, _progressListener: ProgressListener): Promise<Unlisten> {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
      if (this.listeners.size === 0) window.clearInterval(this.timer);
    };
  }

  async add(input: CreateDownloadInput): Promise<DownloadItem> {
    const duplicate = this.state.downloads.some((item) =>
      item.fileName.toLowerCase() === input.fileName.toLowerCase()
        && item.destination.toLowerCase() === input.destination.toLowerCase(),
    );
    if (duplicate) throw new Error("Ya existe una descarga con el mismo archivo de destino.");
    const now = new Date().toISOString();
    const item: DownloadItem = {
      id: createId(),
      fileName: input.fileName,
      category: input.category || categoryForFile(input.fileName),
      state: input.startImmediately ? { kind: "queued" } : { kind: "paused" },
      source: previewSource(input.source),
      destination: input.destination,
      transfer: {
        downloadedBytes: 0,
        size: { kind: "known", totalBytes: 480_000_000 + Math.round(Math.random() * 1_400_000_000) },
        validator: { kind: "none" },
        resume: { kind: "unknown" },
      },
      telemetry: emptyTelemetry(),
      threads: input.threads,
      speedLimitBytes: input.speedLimitBytes,
      createdAt: now,
      updatedAt: now,
    };
    this.state.downloads.unshift(item);
    this.schedule();
    this.commit();
    return structuredClone(item);
  }

  async control(idValue: string, action: DownloadAction): Promise<void> {
    const item = this.requireDownload(idValue);
    if (action === "remove") {
      this.state.downloads = this.state.downloads.filter(({ id: itemId }) => itemId !== idValue);
    } else if (action === "pause") {
      item.state = { kind: "paused" };
      item.telemetry.phase = "idle";
      item.telemetry.segments.forEach((segment) => {
        segment.speedBytes = 0;
        if (segment.state !== "completed") segment.state = "paused";
      });
    } else if (action === "restart") {
      item.transfer.downloadedBytes = 0;
      item.transfer.resume = { kind: "unknown" };
      item.telemetry = emptyTelemetry();
      item.state = { kind: "queued" };
    } else {
      item.telemetry.phase = "idle";
      item.telemetry.segments.forEach((segment) => {
        segment.speedBytes = 0;
        if (segment.state !== "completed") segment.state = "pending";
      });
      item.state = { kind: "queued" };
    }
    item.updatedAt = new Date().toISOString();
    this.schedule();
    this.commit();
  }

  async replaceSource(idValue: string, source: DownloadSource): Promise<void> {
    const item = this.requireDownload(idValue);
    item.source = previewSource(source);
    item.transfer.resume = { kind: "unknown" };
    item.telemetry = emptyTelemetry();
    item.state = { kind: "queued" };
    item.updatedAt = new Date().toISOString();
    this.schedule();
    this.commit();
  }

  async updateSettings(settings: AppSettings): Promise<void> {
    this.state.settings = structuredClone(settings);
    this.schedule();
    this.commit();
  }

  async saveProxy(proxy: ProxyProfile): Promise<void> {
    const safeProxy = previewProxy(proxy);
    const index = this.state.proxies.findIndex(({ id: proxyId }) => proxyId === proxy.id);
    if (index >= 0) this.state.proxies[index] = safeProxy;
    else this.state.proxies.push(safeProxy);
    this.commit();
  }

  async removeProxy(idValue: string): Promise<void> {
    this.state.proxies = this.state.proxies.filter(({ id: proxyId }) => proxyId !== idValue);
    this.commit();
  }

  async checkProxy(idValue: string): Promise<void> {
    const proxy = this.state.proxies.find(({ id: proxyId }) => proxyId === idValue);
    if (!proxy) throw new Error("Perfil de proxy no encontrado.");
    proxy.health = { kind: "checking" };
    this.commit();
    window.setTimeout(() => {
      proxy.health = proxy.enabled
        ? { kind: "online", latencyMs: 42 + Math.round(Math.random() * 80) }
        : { kind: "offline", reason: "El perfil está desactivado" };
      this.commit();
    }, 800);
  }

  private load(): AppSnapshot {
    const saved = localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!saved) return demoSnapshot();
    try {
      const parsed = JSON.parse(saved) as AppSnapshot;
      const sanitized: AppSnapshot = {
        ...parsed,
        settings: {
          ...DEFAULT_SETTINGS,
          ...parsed.settings,
          defaultSpeedLimitBytes: parsed.settings.defaultSpeedLimitBytes ?? 0,
          categoryDirectories: { ...DEFAULT_SETTINGS.categoryDirectories, ...parsed.settings.categoryDirectories },
        },
        downloads: parsed.downloads.map((item) => {
          const restored = {
            ...item,
            state: item.state.kind === "downloading" ? { kind: "queued" as const } : item.state,
            speedLimitBytes: item.speedLimitBytes ?? 0,
            source: previewSource(item.source),
            transfer: { ...item.transfer, resume: item.transfer.resume ?? { kind: "unknown" as const } },
            telemetry: normalizeTelemetry(item.telemetry),
          };
          if (restored.telemetry.segments.length === 0 && restored.transfer.downloadedBytes > 0) {
            restored.telemetry = legacyTelemetry(restored);
          }
          return restored;
        }),
        proxies: parsed.proxies.map(previewProxy),
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(sanitized));
      localStorage.removeItem(LEGACY_STORAGE_KEY);
      return sanitized;
    } catch {
      return demoSnapshot();
    }
  }

  private requireDownload(idValue: string): DownloadItem {
    const item = this.state.downloads.find(({ id: itemId }) => itemId === idValue);
    if (!item) throw new Error("Descarga no encontrada.");
    return item;
  }

  private schedule(): void {
    const active = this.state.downloads.filter(({ state }) => state.kind === "downloading");
    const slots = Math.max(0, this.state.settings.maxConcurrent - active.length);
    this.state.downloads
      .filter(({ state }) => state.kind === "queued")
      .slice(0, slots)
      .forEach((item) => {
        item.state = { kind: "downloading", speedBytes: 4_000_000 + Math.round(Math.random() * 17_000_000) };
        initializePreviewTelemetry(item);
        item.telemetry.phase = "connecting";
        item.telemetry.segments.forEach((segment) => {
          if (segment.state !== "completed") segment.state = "connecting";
        });
      });
  }

  private tick(): void {
    let changed = false;
    for (const item of this.state.downloads) {
      if (advancePreviewDownload(item)) changed = true;
    }
    if (changed) {
      this.schedule();
      this.commit();
    }
  }

  private commit(): void {
    this.state.revision += 1;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(this.state));
    const snapshot = structuredClone(this.state);
    this.listeners.forEach((listener) => listener(snapshot));
  }
}
