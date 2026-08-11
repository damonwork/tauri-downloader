import { computed, onBeforeUnmount, readonly, ref, shallowReadonly } from "vue";
import type { DownloadGateway, Unlisten } from "./download-gateway";
import type { AppSnapshot, AppSettings, ProxyProfile } from "@/domain/settings";
import { DEFAULT_SETTINGS } from "@/domain/settings";
import type { CreateDownloadInput, DownloadAction, DownloadSource } from "@/domain/download";
import { deriveStats } from "@/domain/download";

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
  let unlisten: Unlisten | undefined;

  const stats = computed(() => deriveStats(snapshot.value.downloads));

  async function init(): Promise<void> {
    try {
      unlisten = await gateway.subscribe((next) => {
        if (next.revision >= snapshot.value.revision) snapshot.value = next;
      });
      const initial = await gateway.snapshot();
      if (initial.revision >= snapshot.value.revision) snapshot.value = initial;
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
      snapshot.value = await gateway.snapshot();
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

function messageOf(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "Ocurrió un error inesperado.";
}
