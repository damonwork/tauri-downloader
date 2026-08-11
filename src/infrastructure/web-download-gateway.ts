import type {
  DownloadGateway,
  RuntimeCapabilities,
  SnapshotListener,
  Unlisten,
} from "@/application/download-gateway";
import type { CreateDownloadInput, DownloadAction, DownloadItem, DownloadSource } from "@/domain/download";
import { categoryForFile } from "@/domain/download";
import { createId } from "@/domain/id";
import { DEFAULT_SETTINGS, type AppSettings, type AppSnapshot, type ProxyProfile } from "@/domain/settings";

const STORAGE_KEY = "fluxor.preview.snapshot.v1";

function demoSnapshot(): AppSnapshot {
  const now = Date.now();
  const source = (url: string): DownloadSource => ({ url, headers: [], cookies: [], proxy: { kind: "direct" } });
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
        transfer: { downloadedBytes: 2_612_000_000, size: { kind: "known", totalBytes: 3_842_000_000 }, validator: { kind: "etag", value: "demo-etag" }, resume: { kind: "supported" } }, threads: 12, speedLimitBytes: 0,
        createdAt: new Date(now - 2_820_000).toISOString(), updatedAt: new Date().toISOString(),
      },
      {
        id: createId(), fileName: "curso-arquitectura-modular.mkv", category: "video", state: { kind: "paused" },
        source: source("https://media.example.net/curso-arquitectura-modular.mkv"), destination: "Fluxor/Videos",
        transfer: { downloadedBytes: 672_000_000, size: { kind: "known", totalBytes: 1_460_000_000 }, validator: { kind: "none" }, resume: { kind: "supported" } }, threads: 8, speedLimitBytes: 5 * 1024 ** 2,
        createdAt: new Date(now - 7_200_000).toISOString(), updatedAt: new Date(now - 900_000).toISOString(),
      },
      {
        id: createId(), fileName: "dataset-2026-08.tar.gz", category: "archive", state: { kind: "queued" },
        source: source("https://data.example.org/dataset-2026-08.tar.gz"), destination: "Fluxor/Comprimidos",
        transfer: { downloadedBytes: 0, size: { kind: "known", totalBytes: 824_000_000 }, validator: { kind: "none" }, resume: { kind: "unknown" } }, threads: 6, speedLimitBytes: 0,
        createdAt: new Date(now - 420_000).toISOString(), updatedAt: new Date(now - 420_000).toISOString(),
      },
      {
        id: createId(), fileName: "manual-servicio.pdf", category: "document", state: { kind: "completed", completedAt: new Date(now - 82_800_000).toISOString() },
        source: source("https://docs.example.com/manual-servicio.pdf"), destination: "Fluxor/Documentos",
        transfer: { downloadedBytes: 28_400_000, size: { kind: "known", totalBytes: 28_400_000 }, validator: { kind: "none" }, resume: { kind: "unsupported", reason: "El servidor no acepta solicitudes por rango" } }, threads: 4, speedLimitBytes: 0,
        createdAt: new Date(now - 86_400_000).toISOString(), updatedAt: new Date(now - 82_800_000).toISOString(),
      },
    ],
  };
}

export class WebDownloadGateway implements DownloadGateway {
  readonly capabilities: RuntimeCapabilities = {
    runtime: "web",
  };

  private state: AppSnapshot;
  private readonly listeners = new Set<SnapshotListener>();
  private readonly timer: number;

  constructor() {
    this.state = this.load();
    this.timer = window.setInterval(() => this.tick(), 900);
  }

  async snapshot(): Promise<AppSnapshot> {
    return structuredClone(this.state);
  }

  async subscribe(listener: SnapshotListener): Promise<Unlisten> {
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
    } else if (action === "restart") {
      item.transfer.downloadedBytes = 0;
      item.transfer.resume = { kind: "unknown" };
      item.state = { kind: "queued" };
    } else {
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
    const saved = localStorage.getItem(STORAGE_KEY);
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
        downloads: parsed.downloads.map((item) => ({
          ...item,
          speedLimitBytes: item.speedLimitBytes ?? 0,
          source: previewSource(item.source),
          transfer: { ...item.transfer, resume: item.transfer.resume ?? { kind: "unknown" } },
        })),
        proxies: parsed.proxies.map(previewProxy),
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(sanitized));
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
      });
  }

  private tick(): void {
    let changed = false;
    for (const item of this.state.downloads) {
      if (item.state.kind !== "downloading" || item.transfer.size.kind !== "known") continue;
      const jitter = 0.72 + Math.random() * 0.56;
      item.state.speedBytes = Math.max(640_000, Math.round(item.state.speedBytes * jitter));
      item.transfer.downloadedBytes = Math.min(
        item.transfer.size.totalBytes,
        item.transfer.downloadedBytes + item.state.speedBytes,
      );
      item.updatedAt = new Date().toISOString();
      if (item.transfer.downloadedBytes >= item.transfer.size.totalBytes) {
        item.state = { kind: "completed", completedAt: new Date().toISOString() };
      }
      changed = true;
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

function previewSource(source: DownloadSource): DownloadSource {
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

function previewProxy(proxy: ProxyProfile): ProxyProfile {
  try {
    const url = new URL(proxy.url);
    url.username = "";
    url.password = "";
    return { ...structuredClone(proxy), url: url.toString() };
  } catch {
    return { ...structuredClone(proxy), url: "invalid-proxy://redacted" };
  }
}
