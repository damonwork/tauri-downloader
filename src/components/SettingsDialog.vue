<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import type {
  BrowserIntegration,
  DiagnosticLevel,
  DiagnosticSnapshot,
} from "@/application/download-gateway";
import type { AppSettings, CategoryDirectories } from "@/domain/settings";
import AppIcon from "./AppIcon.vue";
import SpeedLimitInput from "./SpeedLimitInput.vue";

const props = defineProps<{
  open: boolean;
  settings: AppSettings;
  busy: boolean;
  browserIntegration: BrowserIntegration;
  diagnostics: DiagnosticSnapshot;
  diagnosticsLoading: boolean;
}>();
const emit = defineEmits<{
  close: [];
  save: [settings: AppSettings];
  refreshDiagnostics: [];
  clearDiagnostics: [];
}>();
const form = reactive<AppSettings>(cloneSettings(props.settings));
const activeSection = ref<"general" | "folders" | "diagnostics">("general");
const tokenCopied = ref(false);
const tokenCopyError = ref(false);
const logLevel = ref<DiagnosticLevel>("info");
const logsCopied = ref(false);
const logsCopyError = ref(false);
const levelRank: Record<DiagnosticLevel, number> = { debug: 0, info: 1, warning: 2, error: 3 };
const levelLabel: Record<DiagnosticLevel, string> = {
  debug: "DEBUG",
  info: "INFO",
  warning: "AVISO",
  error: "ERROR",
};
const categoryFields: { key: keyof CategoryDirectories; label: string; description: string }[] = [
  { key: "video", label: "Videos", description: "MP4, MKV, MOV, WEBM" },
  { key: "archive", label: "Comprimidos", description: "ZIP, RAR, 7Z, TAR" },
  { key: "document", label: "Documentos", description: "PDF, DOCX, XLSX, TXT" },
  { key: "audio", label: "Audio", description: "MP3, FLAC, WAV, M4A" },
  { key: "other", label: "Otros", description: "Archivos sin categoría" },
];

const rootIsAbsolute = computed(() => /^[a-zA-Z]:[\\/]|^\//.test(form.downloadDirectory.trim()));
const rootPreview = computed(() =>
  rootIsAbsolute.value ? form.downloadDirectory : `Descargas / ${form.downloadDirectory}`,
);
const filteredDiagnostics = computed(() =>
  props.diagnostics.entries.filter((entry) => levelRank[entry.level] >= levelRank[logLevel.value]),
);

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    Object.assign(form, cloneSettings(props.settings));
    activeSection.value = "general";
  },
);

function submit(): void {
  emit("save", cloneSettings(form));
}

async function copyBrowserToken(): Promise<void> {
  if (!props.browserIntegration.token) return;
  try {
    await navigator.clipboard.writeText(props.browserIntegration.token);
    tokenCopied.value = true;
    tokenCopyError.value = false;
    window.setTimeout(() => {
      tokenCopied.value = false;
    }, 1800);
  } catch {
    tokenCopyError.value = true;
  }
}

async function copyDiagnosticLogs(): Promise<void> {
  const text =
    filteredDiagnostics.value
      .map((entry) => {
        const details = Object.keys(entry.details).length
          ? ` ${JSON.stringify(entry.details)}`
          : "";
        return `${entry.at} [${levelLabel[entry.level]}] ${entry.scope}/${entry.event}: ${entry.message}${details}`;
      })
      .join("\n") || "Sin eventos para el nivel seleccionado.";
  try {
    await navigator.clipboard.writeText(text);
    logsCopied.value = true;
    logsCopyError.value = false;
  } catch {
    logsCopyError.value = true;
  }
  window.setTimeout(() => {
    logsCopied.value = false;
  }, 1800);
}

function diagnosticTime(value: string): string {
  return new Date(value).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function cloneSettings(settings: AppSettings): AppSettings {
  return {
    maxConcurrent: settings.maxConcurrent,
    defaultThreads: settings.defaultThreads,
    defaultSpeedLimitBytes: settings.defaultSpeedLimitBytes,
    downloadDirectory: settings.downloadDirectory,
    organizeByCategory: settings.organizeByCategory,
    categoryDirectories: { ...settings.categoryDirectories },
    startImmediately: settings.startImmediately,
  };
}
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="dialog-backdrop ui-dialog-backdrop" @mousedown.self="emit('close')">
      <section
        class="settings-card ui-dialog-card h-[min(760px,calc(100dvh-40px))] max-w-4xl max-dialog:h-[94dvh]"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
      >
        <header class="ui-dialog-header">
          <div>
            <p class="ui-eyebrow">PREFERENCIAS</p>
            <h2 class="ui-dialog-title" id="settings-title">Configurar Fluxor</h2>
          </div>
          <button class="ui-icon-button" type="button" aria-label="Cerrar" @click="emit('close')">
            <AppIcon name="close" />
          </button>
        </header>

        <div
          class="settings-layout grid min-h-0 flex-1 grid-cols-[174px_minmax(0,1fr)] overflow-hidden max-[620px]:flex max-[620px]:flex-col"
        >
          <nav
            class="shrink-0 border-r border-line bg-canvas p-3 max-[620px]:grid max-[620px]:grid-cols-3 max-[620px]:gap-2 max-[620px]:border-r-0 max-[620px]:border-b"
            aria-label="Secciones de preferencias"
          >
            <button
              class="ui-settings-tab"
              type="button"
              :class="{ active: activeSection === 'general' }"
              @click="activeSection = 'general'"
            >
              <AppIcon name="settings" :size="17" /><span
                ><strong class="block text-[inherit]">General</strong
                ><small class="mt-[3px] block text-muted max-[620px]:hidden"
                  >Motor y comportamiento</small
                ></span
              >
            </button>
            <button
              class="ui-settings-tab"
              type="button"
              :class="{ active: activeSection === 'folders' }"
              @click="activeSection = 'folders'"
            >
              <AppIcon name="folder" :size="17" /><span
                ><strong class="block text-[inherit]">Carpetas</strong
                ><small class="mt-[3px] block text-muted max-[620px]:hidden"
                  >Destino por categoría</small
                ></span
              >
            </button>
            <button
              class="ui-settings-tab"
              type="button"
              :class="{ active: activeSection === 'diagnostics' }"
              @click="activeSection = 'diagnostics'"
            >
              <AppIcon name="activity" :size="17" /><span
                ><strong class="block text-[inherit]">Diagnóstico</strong
                ><small class="mt-[3px] block text-muted max-[620px]:hidden"
                  >Eventos y errores</small
                ></span
              >
            </button>
          </nav>

          <form
            class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
            @submit.prevent="submit"
          >
            <div class="settings-body min-h-0 flex-1 overflow-y-auto p-6 max-dialog:p-4">
              <section v-if="activeSection === 'general'">
                <div class="section-heading mb-[13px] flex items-center gap-[11px]">
                  <span
                    class="grid h-9 w-9 place-items-center rounded-lg bg-[rgba(194,255,91,.08)] text-accent"
                    ><AppIcon name="bolt" :size="18"
                  /></span>
                  <div>
                    <h3 class="m-0 text-sm">Motor de transferencia</h3>
                    <p class="[margin:4px_0_0] text-muted">
                      Valores predeterminados para nuevas descargas.
                    </p>
                  </div>
                </div>
                <div
                  class="setting-row flex min-h-20 items-center justify-between gap-6 border-b border-line py-4 max-dialog:flex-col max-dialog:items-stretch max-dialog:gap-3"
                >
                  <div>
                    <strong class="block text-caption">Descargas simultáneas</strong>
                    <p class="[margin:4px_0_0] text-muted">
                      Archivos que pueden transferirse al mismo tiempo.
                    </p>
                  </div>
                  <div
                    class="stepper grid h-10 w-fit shrink-0 grid-cols-[40px_64px_40px] overflow-hidden rounded-lg border border-line bg-canvas"
                  >
                    <button
                      class="grid size-10 place-items-center text-lg text-muted hover:bg-elevated hover:text-accent"
                      type="button"
                      @click="form.maxConcurrent = Math.max(1, form.maxConcurrent - 1)"
                    >
                      −</button
                    ><input
                      class="h-10 min-w-0 border-x border-line bg-canvas text-center font-mono text-sm text-foreground"
                      v-model.number="form.maxConcurrent"
                      name="max-concurrent"
                      type="number"
                      min="1"
                      max="12"
                    /><button
                      class="grid size-10 place-items-center text-lg text-muted hover:bg-elevated hover:text-accent"
                      type="button"
                      @click="form.maxConcurrent = Math.min(12, form.maxConcurrent + 1)"
                    >
                      +
                    </button>
                  </div>
                </div>
                <div
                  class="setting-row flex min-h-20 items-center justify-between gap-6 border-b border-line py-4 max-dialog:flex-col max-dialog:items-stretch max-dialog:gap-3"
                >
                  <div>
                    <strong class="block text-caption">Máximo de segmentos</strong>
                    <p class="[margin:4px_0_0] text-muted">
                      Es un límite: Fluxor usa menos si el servidor no los tolera.
                    </p>
                  </div>
                  <select
                    class="ui-input"
                    v-model.number="form.defaultThreads"
                    name="default-threads"
                  >
                    <option
                      v-for="value in [1, 2, 4, 6, 8, 12, 16, 24, 32]"
                      :key="value"
                      :value="value"
                    >
                      Hasta {{ value }}
                    </option>
                  </select>
                </div>
                <div
                  class="setting-row flex min-h-20 items-center justify-between gap-6 border-b border-line py-4 max-dialog:flex-col max-dialog:items-stretch max-dialog:gap-3"
                >
                  <div>
                    <strong class="block text-caption">Límite de velocidad</strong>
                    <p class="[margin:4px_0_0] text-muted">
                      Tope agregado predeterminado para cada descarga.
                    </p>
                  </div>
                  <SpeedLimitInput
                    v-model="form.defaultSpeedLimitBytes"
                    class="setting-speed w-47.5 [flex:0_0_190px] max-[620px]:w-42.5 max-[620px]:[flex-basis:170px]"
                    name="default-speed-limit"
                  />
                </div>
                <label
                  class="toggle-row relative flex min-h-18 cursor-pointer items-center justify-between gap-5 border-b border-b-line"
                  ><div>
                    <strong class="block text-caption">Iniciar al añadir</strong>
                    <p class="[margin:4px_0_0] text-muted">
                      Las nuevas transferencias entran directamente en la cola.
                    </p>
                  </div>
                  <input
                    class="absolute opacity-0"
                    v-model="form.startImmediately"
                    name="start-immediately"
                    type="checkbox" /><i
                    class="h-[19px] w-8.5 [flex:0_0_auto] rounded-xl bg-line p-[3px] after:block after:h-[13px] after:w-[13px] after:rounded-full after:bg-muted after:content-[''] after:[transition:.2s] [.toggle-row_input:checked+&]:bg-accent [.toggle-row_input:checked+&::after]:[transform:translateX(15px)] [.toggle-row_input:checked+&::after]:bg-surface"
                  ></i
                ></label>
                <div
                  class="info-card mt-5 flex gap-2.5 rounded-lg border border-line bg-[rgba(194,255,91,.025)] p-[13px] text-accent"
                >
                  <AppIcon name="activity" :size="18" />
                  <p class="m-0 leading-[1.5] text-muted">
                    <strong class="mb-0.5 block text-muted">Configuración individual</strong>Cada
                    descarga puede sobrescribir categoría, destino, segmentos, proxy, cookies y
                    headers desde el diálogo de alta.
                  </p>
                </div>
                <div
                  class="browser-card mt-4.5 rounded-lg border border-line bg-[linear-gradient(135deg,rgba(194,255,91,.065),rgba(194,255,91,.018))] px-[15px] py-3.5 shadow-[inset_0_1px_rgba(255,255,255,.025)]"
                >
                  <div class="browser-card-header flex items-center gap-2.5">
                    <span
                      class="browser-card-icon grid h-[31px] w-[31px] [flex:0_0_31px] place-items-center rounded-lg bg-[rgba(194,255,91,.1)] text-accent"
                      ><AppIcon name="link" :size="17"
                    /></span>
                    <div class="browser-card-title min-w-0 flex-1">
                      <p class="[margin:0_0_3px] font-mono tracking-[.12em] text-muted">
                        PUENTE LOCAL
                      </p>
                      <strong class="block text-foreground">Extensión del navegador</strong>
                    </div>
                    <span
                      class="browser-status flex items-center gap-[5px] font-mono whitespace-nowrap text-success [&.offline]:text-warning"
                      :class="{ offline: !browserIntegration.available }"
                      ><i
                        class="h-1.5 w-1.5 rounded-full bg-accent shadow-[0_0_8px_var(--color-accent)] [.browser-status.offline_&]:bg-danger [.browser-status.offline_&]:shadow-[none]"
                      ></i
                      >{{ browserIntegration.available ? "Disponible" : "No disponible" }}</span
                    >
                  </div>
                  <p class="browser-card-copy mx-0 my-3 leading-[1.45] text-muted">
                    Conecta Chrome o Firefox para enviar descargas directamente a la cola de Fluxor.
                  </p>
                  <div
                    v-if="browserIntegration.available"
                    class="browser-token flex min-w-0 items-center gap-[7px]"
                  >
                    <code
                      class="min-w-0 flex-1 [cursor:text] overflow-hidden rounded-lg border border-line bg-canvas px-2.5 py-[9px] font-mono tracking-[.02em] text-ellipsis whitespace-nowrap text-success [user-select:all]"
                      tabindex="0"
                      title="Selecciona el token para copiarlo manualmente"
                      >{{ browserIntegration.token }}</code
                    >
                    <button
                      class="flex h-[31px] cursor-pointer items-center gap-[5px] rounded-lg border border-line bg-surface px-[9px] py-0 font-mono whitespace-nowrap text-success hover:border-accent hover:text-accent"
                      type="button"
                      @click="copyBrowserToken"
                    >
                      <AppIcon :name="tokenCopied ? 'check' : 'link'" :size="14" />{{
                        tokenCopied ? "Copiado" : "Copiar token"
                      }}
                    </button>
                  </div>
                  <small
                    v-if="tokenCopyError"
                    class="browser-copy-error mt-[7px] block font-mono leading-[1.4] text-danger"
                    >No se pudo copiar automáticamente. Selecciona el token y cópialo
                    manualmente.</small
                  >
                  <p v-else class="browser-offline mx-0 my-2.5 text-danger">
                    Inicia el motor de escritorio para activar la conexión.
                  </p>
                  <small class="browser-card-note mt-2.5 block font-mono leading-[1.4] text-muted"
                    >El token se guarda localmente y solo se usa para autorizar el puente en este
                    equipo.</small
                  >
                </div>
              </section>

              <section v-else-if="activeSection === 'folders'">
                <div class="section-heading mb-[13px] flex items-center gap-[11px]">
                  <span
                    class="grid h-9 w-9 place-items-center rounded-lg bg-[rgba(194,255,91,.08)] text-accent"
                    ><AppIcon name="folder" :size="18"
                  /></span>
                  <div>
                    <h3 class="m-0 text-sm">Organización de archivos</h3>
                    <p class="[margin:4px_0_0] text-muted">
                      La raíz relativa se crea dentro de Descargas del sistema.
                    </p>
                  </div>
                </div>
                <label
                  class="root-field mt-4.5 block [&_.app-icon]:absolute [&_.app-icon]:top-2.5 [&_.app-icon]:left-2.5 [&_.app-icon]:text-muted"
                  ><span class="ui-label">CARPETA PRINCIPAL</span>
                  <div class="relative">
                    <AppIcon name="folder" :size="16" /><input
                      class="ui-input w-full"
                      v-model="form.downloadDirectory"
                      name="download-directory"
                      placeholder="Fluxor o C:\\Mi ruta"
                    />
                  </div>
                  <small class="mt-1.5 block font-mono text-muted"
                    >Resultado: {{ rootPreview }}</small
                  ></label
                >
                <label
                  class="toggle-row organize-toggle relative mt-2 flex min-h-15.5 cursor-pointer items-center justify-between gap-5 border-b border-b-line"
                  ><div>
                    <strong class="block text-caption">Organizar por categoría</strong>
                    <p class="[margin:4px_0_0] text-muted">
                      Crear automáticamente una subcarpeta según el tipo.
                    </p>
                  </div>
                  <input
                    class="absolute opacity-0"
                    v-model="form.organizeByCategory"
                    name="organize-categories"
                    type="checkbox" /><i
                    class="h-[19px] w-8.5 [flex:0_0_auto] rounded-xl bg-line p-[3px] after:block after:h-[13px] after:w-[13px] after:rounded-full after:bg-muted after:content-[''] after:[transition:.2s] [.toggle-row_input:checked+&]:bg-accent [.toggle-row_input:checked+&::after]:[transform:translateX(15px)] [.toggle-row_input:checked+&::after]:bg-surface"
                  ></i
                ></label>
                <div
                  class="category-folders mt-2.5 overflow-hidden rounded-lg border border-line [transition:opacity_.2s] [&.disabled]:opacity-42"
                  :class="{ disabled: !form.organizeByCategory }"
                >
                  <label
                    class="grid min-h-[53px] grid-cols-[29px_1fr_10px_135px] items-center gap-2 border-b border-b-line px-[9px] py-[7px] last:border-0 max-[620px]:grid-cols-[29px_1fr_8px_105px]"
                    v-for="field in categoryFields"
                    :key="field.key"
                    ><span
                      class="category-icon grid h-[27px] w-[27px] place-items-center rounded-lg bg-surface text-muted"
                      ><AppIcon name="folder" :size="15" /></span
                    ><span class="category-copy"
                      ><strong class="block text-caption">{{ field.label }}</strong
                      ><small class="mt-0.5 block text-muted">{{ field.description }}</small></span
                    ><span class="path-prefix font-mono text-muted">/</span
                    ><input
                      class="ui-input w-full"
                      v-model="form.categoryDirectories[field.key]"
                      :name="`folder-${field.key}`"
                      :disabled="!form.organizeByCategory"
                  /></label>
                </div>
              </section>
              <section v-else class="diagnostics-section">
                <div class="section-heading mb-[13px] flex items-center gap-[11px]">
                  <span
                    class="grid h-9 w-9 place-items-center rounded-lg bg-[rgba(194,255,91,.08)] text-accent"
                    ><AppIcon name="activity" :size="18"
                  /></span>
                  <div>
                    <h3 class="m-0 text-sm">Registro de diagnóstico</h3>
                    <p class="[margin:4px_0_0] text-muted">
                      Flujo del puente, la cola y las transferencias.
                    </p>
                  </div>
                </div>
                <div class="diagnostic-toolbar flex flex-wrap items-end justify-between gap-3">
                  <label class="block"
                    ><span class="ui-label">NIVEL MÍNIMO</span
                    ><select class="ui-input" v-model="logLevel">
                      <option value="debug">Debug</option>
                      <option value="info">Info</option>
                      <option value="warning">Aviso</option>
                      <option value="error">Error</option>
                    </select></label
                  >
                  <div class="flex gap-1.5">
                    <button
                      class="flex h-[35px] cursor-pointer items-center gap-1.5 rounded-lg border border-line bg-raised px-[11px] py-0 text-muted [transition:border-color_.15s,color_.15s] hover:border-line hover:text-foreground"
                      type="button"
                      @click="emit('refreshDiagnostics')"
                    >
                      <AppIcon name="refresh" :size="13" />{{
                        diagnosticsLoading ? "Actualizando…" : "Actualizar"
                      }}</button
                    ><button
                      class="flex h-[35px] cursor-pointer items-center gap-1.5 rounded-lg border border-line bg-raised px-[11px] py-0 text-muted [transition:border-color_.15s,color_.15s] hover:border-line hover:text-foreground"
                      type="button"
                      @click="copyDiagnosticLogs"
                    >
                      <AppIcon name="link" :size="13" />{{
                        logsCopied ? "Copiado" : "Copiar"
                      }}</button
                    ><button
                      class="danger flex h-[35px] cursor-pointer items-center gap-1.5 rounded-lg border border-line bg-raised px-[11px] py-0 text-danger [transition:border-color_.15s,color_.15s] hover:border-danger/40 hover:text-warning"
                      type="button"
                      @click="emit('clearDiagnostics')"
                    >
                      <AppIcon name="trash" :size="13" />Limpiar
                    </button>
                  </div>
                </div>
                <p
                  v-if="logsCopyError"
                  class="diagnostic-copy-error [margin:10px_0_0] block font-mono text-danger"
                >
                  No se pudieron copiar los logs al portapapeles.
                </p>
                <div
                  class="diagnostic-summary mt-3.5 flex min-h-8.5 items-center justify-between gap-3"
                >
                  <span class="flex items-center gap-[7px] whitespace-nowrap text-muted"
                    ><i
                      class="h-1.5 w-1.5 rounded-full bg-accent shadow-[0_0_8px_var(--color-accent)]"
                    ></i
                    >{{ filteredDiagnostics.length }} eventos visibles</span
                  ><small
                    class="ml-auto text-right font-mono leading-[1.4] text-muted max-[620px]:hidden"
                    >Se conservan hasta {{ diagnostics.maxEntries }} eventos. Tokens, cookies y
                    firmas se excluyen.</small
                  >
                </div>
                <div
                  class="diagnostic-list mt-2.5 overflow-hidden rounded-lg border border-line bg-canvas"
                  role="log"
                  aria-label="Registro de diagnóstico"
                >
                  <article
                    class="border-b border-b-line px-3 py-2.5 last:border-0"
                    v-for="entry in filteredDiagnostics"
                    :key="entry.id"
                    :class="entry.level"
                  >
                    <div class="flex min-w-0 items-center gap-2 max-[620px]:flex-wrap">
                      <span
                        class="min-w-11 rounded-md bg-surface px-0 py-[3px] text-center font-mono tracking-[.08em] text-success [.diagnostic-list_article.debug>div>&]:bg-surface [.diagnostic-list_article.debug>div>&]:text-muted [.diagnostic-list_article.error>div>&]:bg-line [.diagnostic-list_article.error>div>&]:text-danger [.diagnostic-list_article.warning>div>&]:bg-line [.diagnostic-list_article.warning>div>&]:text-warning"
                        >{{ levelLabel[entry.level] }}</span><time
                        class="ml-auto font-mono whitespace-nowrap text-muted max-[620px]:[order:3] max-[620px]:ml-0"
                        >{{ diagnosticTime(entry.at) }}</time
                      ><code
                        class="max-w-[42%] overflow-hidden font-mono text-ellipsis whitespace-nowrap text-muted max-[620px]:[order:4] max-[620px]:max-w-full"
                        >{{ entry.scope }}/{{ entry.event }}</code
                      >
                    </div>
                    <p class="[margin:7px_0_0] leading-[1.5] wrap-anywhere text-foreground">
                      {{ entry.message }}
                    </p>
                    <dl
                      class="[margin:7px_0_0] grid grid-cols-[auto_1fr] gap-[3px_12px] border-t border-t-line pt-[7px]"
                      v-if="Object.keys(entry.details).length"
                    >
                      <template v-for="(value, key) in entry.details" :key="key"
                        ><dt class="font-mono text-muted">{{ key }}</dt>
                        <dd
                          class="m-0 min-w-0 overflow-hidden font-mono text-ellipsis whitespace-nowrap text-muted"
                          :title="String(value)"
                        >
                          {{ value }}
                        </dd></template
                      >
                    </dl>
                  </article>
                  <div
                    v-if="!filteredDiagnostics.length"
                    class="diagnostic-empty flex flex-col items-center gap-1.5 px-4.5 py-9 text-center [&_.app-icon]:text-line"
                  >
                    <AppIcon name="activity" :size="22" /><strong class="text-muted"
                      >Sin eventos para este nivel</strong
                    >
                    <p class="m-0 leading-[1.5] text-muted">
                      Usa la extensión o inicia una descarga para generar actividad.
                    </p>
                  </div>
                </div>
              </section>
            </div>

            <footer class="ui-dialog-footer flex-wrap">
              <p
                class="m-0 flex items-center gap-1.5 text-muted max-[620px]:hidden"
                v-if="activeSection === 'folders'"
              >
                <AppIcon name="shield" :size="14" />Las rutas se validan antes de crear archivos.
              </p>
              <p
                class="m-0 flex items-center gap-1.5 text-muted max-[620px]:hidden"
                v-else-if="activeSection === 'diagnostics'"
              >
                <AppIcon name="shield" :size="14" />Los diagnósticos pueden incluir rutas locales
                para investigar fallos de apertura.
              </p>
              <div class="ml-auto flex gap-[7px]">
                <button class="ui-button" type="button" @click="emit('close')">
                  {{ activeSection === "diagnostics" ? "Cerrar" : "Cancelar" }}</button><button
                  v-if="activeSection !== 'diagnostics'"
                  class="save ui-button ui-button-primary"
                  type="submit"
                  :disabled="busy"
                >
                  {{ busy ? "Guardando…" : "Guardar cambios" }}
                </button>
              </div>
            </footer>
          </form>
        </div>
      </section>
    </div>
  </Teleport>
</template>
