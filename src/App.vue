<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { CreateDownloadInput, DownloadAction, DownloadItem, DownloadSource } from "@/domain/download";
import type { AppSettings, ProxyProfile } from "@/domain/settings";
import { hostOf } from "@/domain/format";
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
import AppIcon from "@/components/AppIcon.vue";

type Filter = "all" | "active" | "queued" | "completed";

const manager = useDownloadManager(createDownloadGateway());
const { snapshot, stats, loading, busy, lastError, capabilities } = manager;
const filter = ref<Filter>("all");
const search = ref("");
const selectedId = ref<string>();
const showAdd = ref(false);
const showSettings = ref(false);
const showProxies = ref(false);
const noticeVisible = ref(capabilities.runtime === "web");

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
  if (action === "remove" && !window.confirm("¿Eliminar esta descarga y sus archivos parciales?")) return;
  await manager.control(id, action);
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

async function removeProxy(id: string): Promise<void> {
  if (window.confirm("¿Eliminar este perfil de proxy?")) await manager.removeProxy(id);
}

function setFilter(value: string): void {
  if (["all", "active", "queued", "completed"].includes(value)) filter.value = value as Filter;
}

function keyboardShortcuts(event: KeyboardEvent): void {
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
  await manager.init();
});
onBeforeUnmount(() => window.removeEventListener("keydown", keyboardShortcuts));
</script>

<template>
  <div class="app-shell">
    <SideRail :active="filter" :counts="counts" @filter="setFilter" @settings="showSettings=true" @proxies="showProxies=true" />
    <main class="workspace">
      <TopBar v-model:search="search" :web-mode="capabilities.runtime === 'web'" @add="showAdd=true" @settings="showSettings=true" @proxies="showProxies=true" />

      <div v-if="noticeVisible" class="runtime-notice">
        <AppIcon name="bolt" :size="16" />
        <p><strong>Vista funcional del navegador</strong>La cola y la configuración se guardan localmente; transferencias, cookies y proxies reales se ejecutan exclusivamente en el motor Rust.</p>
        <button type="button" aria-label="Ocultar aviso" @click="noticeVisible=false"><AppIcon name="close" :size="15" /></button>
      </div>

      <div v-if="lastError.kind === 'message'" class="error-banner">
        <AppIcon name="warning" :size="16" /><p>{{ lastError.message }}</p><button type="button" @click="lastError={kind:'none'}"><AppIcon name="close" :size="15" /></button>
      </div>

      <SummaryStrip :stats="stats" :max-concurrent="snapshot.settings.maxConcurrent" />
      <DownloadList
        :items="filteredDownloads"
        :selected-id="selectedId"
        :filter-label="filterLabel"
        @select="selectedId=$event"
        @action="control"
        @add="showAdd=true"
      />

      <div v-if="loading" class="loading-state"><span></span><p>Sincronizando el motor…</p></div>
    </main>

    <DetailPanel
      v-if="selectedItem"
      :item="selectedItem"
      @close="selectedId=undefined"
      @action="control(selectedItem.id,$event)"
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
    <SettingsDialog :open="showSettings" :settings="snapshot.settings" :busy="busy" @close="showSettings=false" @save="saveSettings" />
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
  </div>
</template>
