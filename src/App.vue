<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { CreateDownloadInput, DownloadAction, DownloadItem, DownloadSource } from "@/domain/download";
import type { AppSettings, ProxyProfile } from "@/domain/settings";
import { formatBytes, hostOf } from "@/domain/format";
import { createDownloadGateway } from "@/infrastructure/runtime";
import { useDownloadManager } from "@/application/use-download-manager";
import SideRail from "@/components/SideRail.vue";
import TopBar from "@/components/TopBar.vue";
import SummaryStrip from "@/components/SummaryStrip.vue";
import DownloadList from "@/components/DownloadList.vue";
import DetailPanel from "@/components/DetailPanel.vue";
import AddDownloadDialog from "@/components/AddDownloadDialog.vue";
import SettingsDialog from "@/components/SettingsDialog.vue";
import ProxyDialog from "@/components/ProxyDialog.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import AppIcon from "@/components/AppIcon.vue";

type Filter = "all" | "active" | "queued" | "completed";
type Confirmation = {
  eyebrow: string;
  title: string;
  message: string;
  confirmLabel: string;
  tone: "warning" | "danger";
  action: () => Promise<unknown>;
};

const manager = useDownloadManager(createDownloadGateway());
const { snapshot, stats, loading, initializationFailed, busy, lastError, capabilities } = manager;
const filter = ref<Filter>("all");
const search = ref("");
const selectedId = ref<string>();
const showAdd = ref(false);
const showSettings = ref(false);
const showProxies = ref(false);
const noticeVisible = ref(capabilities.runtime === "web");
const confirmation = ref<Confirmation>();
const confirming = ref(false);
const browserIntegration = ref({ available: false, port: 17846, token: "" });

const counts = computed(() => ({
  all: snapshot.value.downloads.length,
  active: snapshot.value.downloads.filter((item) => item.state.kind === "downloading").length,
  queued: snapshot.value.downloads.filter((item) => item.state.kind === "queued" || item.state.kind === "paused").length,
  completed: snapshot.value.downloads.filter((item) => item.state.kind === "completed").length,
}));

const filteredDownloads = computed(() => {
  const query = search.value.trim().toLowerCase();
  return snapshot.value.downloads.filter((item) => {
    const matchesFilter = filter.value === "all"
      || (filter.value === "active" && item.state.kind === "downloading")
      || (filter.value === "queued" && (item.state.kind === "queued" || item.state.kind === "paused"))
      || (filter.value === "completed" && item.state.kind === "completed");
    const matchesSearch = !query
      || item.fileName.toLowerCase().includes(query)
      || hostOf(item.source.url).toLowerCase().includes(query);
    return matchesFilter && matchesSearch;
  });
});

const selectedItem = computed<DownloadItem | undefined>(() =>
  snapshot.value.downloads.find((item) => item.id === selectedId.value),
);

const filterLabel = computed(() => ({
  all: "Todas las descargas",
  active: "Transferencias activas",
  queued: "En espera",
  completed: "Descargas completadas",
})[filter.value]);

watch(() => snapshot.value.downloads, (downloads) => {
  if (selectedId.value && !downloads.some((item) => item.id === selectedId.value)) selectedId.value = undefined;
});

async function addDownload(input: CreateDownloadInput): Promise<void> {
  if (await manager.add(input)) showAdd.value = false;
}

async function control(id: string, action: DownloadAction): Promise<void> {
  const item = snapshot.value.downloads.find((download) => download.id === id);
  if (action === "remove") {
    requestConfirmation({
      eyebrow: "ELIMINAR TRANSFERENCIA",
      title: "¿Eliminar esta descarga?",
      message: `Se quitará ${item?.fileName ?? "esta descarga"} de la cola y también se borrarán sus archivos parciales. Esta acción no se puede deshacer.`,
      confirmLabel: "Eliminar descarga",
      tone: "danger",
      action: () => manager.control(id, action),
    });
    return;
  }
  if (action === "pause" && item?.transfer.resume.kind === "unsupported") {
    requestConfirmation({
      eyebrow: "PROGRESO EN RIESGO",
      title: "¿Pausar sin reanudación?",
      message: "Este servidor no acepta solicitudes por rango. Si pausas ahora, tendrás que reiniciar la transferencia desde cero.",
      confirmLabel: "Pausar de todos modos",
      tone: "warning",
      action: () => manager.control(id, action),
    });
    return;
  }
  if (action === "restart" && item && item.transfer.downloadedBytes > 0) {
    requestConfirmation({
      eyebrow: "REINICIAR DESDE CERO",
      title: "¿Descartar el progreso descargado?",
      message: `Se eliminarán ${formatBytes(item.transfer.downloadedBytes)} de datos parciales de ${item.fileName}. Esta acción no se puede deshacer.`,
      confirmLabel: "Reiniciar desde cero",
      tone: "danger",
      action: () => manager.control(id, action),
    });
    return;
  }
  await manager.control(id, action);
}

async function revealDownload(id: string): Promise<void> {
  await manager.revealDownload(id);
}

async function replaceSource(id: string, source: DownloadSource): Promise<void> {
  await manager.replaceSource(id, source);
}

async function saveSettings(settings: AppSettings): Promise<void> {
  if (await manager.updateSettings(settings)) showSettings.value = false;
}

async function saveProxy(proxy: ProxyProfile): Promise<void> {
  await manager.saveProxy(proxy);
}

function removeProxy(id: string): void {
  const proxy = snapshot.value.proxies.find((profile) => profile.id === id);
  requestConfirmation({
    eyebrow: "ELIMINAR PERFIL",
    title: "¿Eliminar este proxy?",
    message: `El perfil ${proxy?.name ?? "seleccionado"} desaparecerá de la configuración de red.`,
    confirmLabel: "Eliminar perfil",
    tone: "danger",
    action: () => manager.removeProxy(id),
  });
}

function requestConfirmation(request: Confirmation): void {
  confirmation.value = request;
}

function cancelConfirmation(): void {
  if (!confirming.value) confirmation.value = undefined;
}

async function confirmAction(): Promise<void> {
  const request = confirmation.value;
  if (!request || confirming.value) return;
  confirming.value = true;
  try {
    await request.action();
  } finally {
    confirming.value = false;
    confirmation.value = undefined;
  }
}

function setFilter(value: string): void {
  if (["all", "active", "queued", "completed"].includes(value)) filter.value = value as Filter;
}

function keyboardShortcuts(event: KeyboardEvent): void {
  if (confirmation.value) return;
  if (!(event.metaKey || event.ctrlKey)) return;
  if (event.key.toLowerCase() === "n") {
    event.preventDefault();
    showAdd.value = true;
  }
  if (event.key.toLowerCase() === "k") {
    event.preventDefault();
    document.querySelector<HTMLInputElement>(".search-box input")?.focus();
  }
}

onMounted(async () => {
  window.addEventListener("keydown", keyboardShortcuts);
  browserIntegration.value = await manager.browserIntegration();
  await manager.init();
});
onBeforeUnmount(() => window.removeEventListener("keydown", keyboardShortcuts));
</script>

<template>
  <div class="app-shell">
    <SideRail :active="filter" :counts="counts" @filter="setFilter" @settings="showSettings=true" @proxies="showProxies=true" />
    <main class="workspace" :class="{ 'inspector-open': selectedItem }">
      <TopBar v-model:search="search" :web-mode="capabilities.runtime === 'web'" @add="showAdd=true" @settings="showSettings=true" @proxies="showProxies=true" />

      <div v-if="noticeVisible" class="runtime-notice">
        <AppIcon name="bolt" :size="16" />
        <p><strong>Vista funcional del navegador</strong>La cola y la configuración se guardan localmente; transferencias, cookies y proxies reales se ejecutan exclusivamente en el motor Rust.</p>
        <button type="button" aria-label="Ocultar aviso" @click="noticeVisible=false"><AppIcon name="close" :size="15" /></button>
      </div>

      <div v-if="lastError.kind === 'message'" class="error-banner" role="alert">
        <AppIcon name="warning" :size="16" /><p>{{ lastError.message }}</p><button type="button" @click="lastError={kind:'none'}"><AppIcon name="close" :size="15" /></button>
      </div>

      <SummaryStrip :stats="stats" :max-concurrent="snapshot.settings.maxConcurrent" />
      <div v-if="initializationFailed" class="connection-state">
        <AppIcon name="warning" :size="25" />
        <h2>No se pudo conectar con el motor</h2>
        <p>La cola guardada no está disponible todavía. Tus descargas no se han eliminado.</p>
        <button type="button" :disabled="loading" @click="manager.retryInit()">{{ loading ? 'Conectando…' : 'Reintentar conexión' }}</button>
      </div>
      <DownloadList
        v-else
        :items="filteredDownloads"
        :selected-id="selectedId"
        :filter-label="filterLabel"
        :can-reveal="capabilities.canRevealDownloads"
        :has-items="snapshot.downloads.length > 0"
        :search-active="search.trim().length > 0"
        @select="selectedId=$event"
        @action="control"
        @reveal="revealDownload"
        @add="showAdd=true"
      />

      <div v-if="loading && !initializationFailed" class="loading-state"><span></span><p>Sincronizando el motor…</p></div>
    </main>

    <DetailPanel
      v-if="selectedItem"
      :item="selectedItem"
      :can-reveal="capabilities.canRevealDownloads"
      @close="selectedId=undefined"
      @action="control(selectedItem.id,$event)"
      @reveal="revealDownload(selectedItem.id)"
      @replace="replaceSource(selectedItem.id,$event)"
    />

    <AddDownloadDialog
      :open="showAdd"
      :settings="snapshot.settings"
      :proxies="snapshot.proxies"
      :native-runtime="capabilities.runtime === 'tauri'"
      :busy="busy"
      @close="showAdd=false"
      @submit="addDownload"
    />
    <SettingsDialog :open="showSettings" :settings="snapshot.settings" :busy="busy" :browser-integration="browserIntegration" @close="showSettings=false" @save="saveSettings" />
    <ProxyDialog
      :open="showProxies"
      :profiles="snapshot.proxies"
      :native-runtime="capabilities.runtime === 'tauri'"
      :busy="busy"
      @close="showProxies=false"
      @save="saveProxy"
      @remove="removeProxy"
      @check="manager.checkProxy"
    />
    <ConfirmDialog
      :open="confirmation !== undefined"
      :eyebrow="confirmation?.eyebrow ?? ''"
      :title="confirmation?.title ?? ''"
      :message="confirmation?.message ?? ''"
      :confirm-label="confirmation?.confirmLabel ?? ''"
      :tone="confirmation?.tone ?? 'warning'"
      :busy="confirming"
      @cancel="cancelConfirmation"
      @confirm="confirmAction"
    />
  </div>
</template>
