import { computed, onBeforeUnmount, readonly, ref, shallowReadonly } from "vue";
import type { DownloadGateway, Unlisten } from "./download-gateway";
import type { AppSnapshot, AppSettings, ProxyProfile } from "@/domain/settings";
import { DEFAULT_SETTINGS } from "@/domain/settings";
import type { CreateDownloadInput, DownloadAction, DownloadProgressEvent, DownloadSource } from "@/domain/download";
import { applyDownloadProgress, deriveStats } from "@/domain/download";

const EMPTY_SNAPSHOT: AppSnapshot = {
  revision: 0,
  downloads: [],
  proxies: [],
  settings: { ...DEFAULT_SETTINGS },
};

export function useDownloadManager(gateway: DownloadGateway) {
  const snapshot = ref<AppSnapshot>(structuredClone(EMPTY_SNAPSHOT));
  const loading = ref(true);
  const busy = ref(false);
  const lastError = ref<{ kind: "none" } | { kind: "message"; message: string }>({ kind: "none" });
  const progressRevisions = new Map<string, number>();
  const pendingProgress = new Map<string, DownloadProgressEvent>();
  let snapshotRevision = 0;
  let unlisten: Unlisten | undefined;

  const stats = computed(() => deriveStats(snapshot.value.downloads));

  function applySnapshot(next: AppSnapshot): void {
    if (next.revision < snapshotRevision) return;
    snapshotRevision = next.revision;
    for (const [id, progress] of pendingProgress) {
      if (progress.revision <= next.revision) {
        pendingProgress.delete(id);
      }
    }
    const merged = overlayDownloadProgress(next, pendingProgress.values());
    snapshot.value = merged;
    progressRevisions.clear();
    merged.downloads.forEach((item) => {
      progressRevisions.set(item.id, pendingProgress.get(item.id)?.revision ?? next.revision);
    });
  }

  function applyProgress(progress: DownloadProgressEvent): void {
    if (progress.revision <= (progressRevisions.get(progress.downloadId) ?? 0)) return;
    progressRevisions.set(progress.downloadId, progress.revision);
    pendingProgress.set(progress.downloadId, progress);
    const index = snapshot.value.downloads.findIndex((item) => item.id === progress.downloadId);
    if (index < 0) {
      void refreshSnapshot();
      return;
    }
    const downloads = applyDownloadProgress(snapshot.value.downloads, progress);
    snapshot.value = {
      ...snapshot.value,
      revision: Math.max(snapshot.value.revision, progress.revision),
      downloads,
    };
  }

  async function refreshSnapshot(): Promise<void> {
    try {
      const next = await gateway.snapshot();
      applySnapshot(next);
    } catch (error) {
      lastError.value = { kind: "message", message: messageOf(error) };
    }
  }

  async function init(): Promise<void> {
    try {
      unlisten = await gateway.subscribe((next) => {
        applySnapshot(next);
      }, applyProgress);
      const initial = await gateway.snapshot();
      applySnapshot(initial);
    } catch (error) {
      lastError.value = { kind: "message", message: messageOf(error) };
    } finally {
      loading.value = false;
    }
  }

  async function execute(operation: () => Promise<unknown>): Promise<boolean> {
    busy.value = true;
    lastError.value = { kind: "none" };
    try {
      await operation();
      applySnapshot(await gateway.snapshot());
      return true;
    } catch (error) {
      lastError.value = { kind: "message", message: messageOf(error) };
      return false;
    } finally {
      busy.value = false;
    }
  }

  async function add(input: CreateDownloadInput): Promise<boolean> {
    return execute(() => gateway.add(input));
  }

  async function control(id: string, action: DownloadAction): Promise<boolean> {
    return execute(() => gateway.control(id, action));
  }

  async function replaceSource(id: string, source: DownloadSource): Promise<boolean> {
    return execute(() => gateway.replaceSource(id, source));
  }

  async function updateSettings(settings: AppSettings): Promise<boolean> {
    return execute(() => gateway.updateSettings(settings));
  }

  async function saveProxy(proxy: ProxyProfile): Promise<boolean> {
    return execute(() => gateway.saveProxy(proxy));
  }

  async function removeProxy(id: string): Promise<boolean> {
    return execute(() => gateway.removeProxy(id));
  }

  async function checkProxy(id: string): Promise<boolean> {
    return execute(() => gateway.checkProxy(id));
  }

  onBeforeUnmount(() => unlisten?.());

  return {
    snapshot: shallowReadonly(snapshot),
    stats,
    loading: readonly(loading),
    busy: readonly(busy),
    lastError,
    capabilities: gateway.capabilities,
    init,
    add,
    control,
    replaceSource,
    updateSettings,
    saveProxy,
    removeProxy,
    checkProxy,
  };
}

export function overlayDownloadProgress(
  snapshot: AppSnapshot,
  events: Iterable<DownloadProgressEvent>,
): AppSnapshot {
  let downloads = snapshot.downloads;
  let revision = snapshot.revision;
  for (const progress of events) {
    if (progress.revision <= snapshot.revision) continue;
    if (!downloads.some((item) => item.id === progress.downloadId)) continue;
    downloads = applyDownloadProgress(downloads, progress);
    revision = Math.max(revision, progress.revision);
  }
  return { ...snapshot, revision, downloads };
}

function messageOf(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "Ocurrió un error inesperado.";
}
