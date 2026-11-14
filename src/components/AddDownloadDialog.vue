<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { CreateDownloadInput, DownloadCategory, ParsedRequest } from "@/domain/download";
import { categoryForFile } from "@/domain/download";
import {
  IngestError,
  normalizeHeaders,
  parseCookies,
  parseHeaderLines,
  parseRequest,
} from "@/domain/ingest";
import type { AppSettings, ProxyProfile } from "@/domain/settings";
import { destinationForCategory } from "@/domain/settings";
import { hostOf } from "@/domain/format";
import AppIcon from "./AppIcon.vue";
import SpeedLimitInput from "./SpeedLimitInput.vue";

const props = defineProps<{
  open: boolean;
  settings: AppSettings;
  proxies: ProxyProfile[];
  nativeRuntime: boolean;
  busy: boolean;
}>();
const emit = defineEmits<{ close: []; submit: [input: CreateDownloadInput] }>();

const rawRequest = ref("");
const fileName = ref("");
const fileNameCustomized = ref(false);
const destination = ref("");
const destinationCustomized = ref(false);
const category = ref<DownloadCategory>("other");
const categoryCustomized = ref(false);
const threads = ref(8);
const singleStream = ref(false);
const speedLimitBytes = ref(0);
const startImmediately = ref(true);
const proxyId = ref("direct");
const extraHeaders = ref("");
const extraCookies = ref("");
const advanced = ref(false);
const submitError = ref("");
const categories: { value: DownloadCategory; label: string }[] = [
  { value: "video", label: "Video" },
  { value: "archive", label: "Comprimido" },
  { value: "document", label: "Documento" },
  { value: "audio", label: "Audio" },
  { value: "other", label: "Otro" },
];
type Analysis =
  | { kind: "empty" }
  | { kind: "valid"; request: ParsedRequest }
  | { kind: "invalid"; message: string };

const analysis = computed<Analysis>(() => {
  if (!rawRequest.value.trim()) return { kind: "empty" };
  try {
    return { kind: "valid", request: parseRequest(rawRequest.value) };
  } catch (error) {
    return {
      kind: "invalid",
      message:
        error instanceof IngestError ? error.message : "No se pudo interpretar la solicitud.",
    };
  }
});
const destinationPreview = computed(() =>
  /^[a-zA-Z]:[\\/]|^\//.test(destination.value.trim())
    ? destination.value
    : `Descargas / ${destination.value}`,
);
const additionalHeaderWarnings = computed(() => {
  try {
    return normalizeHeaders(parseHeaderLines(extraHeaders.value)).warnings;
  } catch {
    return [];
  }
});

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    rawRequest.value = "";
    singleStream.value = false;
    fileName.value = "";
    fileNameCustomized.value = false;
    category.value = "other";
    categoryCustomized.value = false;
    destinationCustomized.value = false;
    destination.value = destinationForCategory(props.settings, category.value);
    threads.value = props.settings.defaultThreads;
    speedLimitBytes.value = props.settings.defaultSpeedLimitBytes;
    startImmediately.value = props.settings.startImmediately;
    proxyId.value = "direct";
    extraHeaders.value = "";
    extraCookies.value = "";
    advanced.value = false;
    submitError.value = "";
  },
);

watch(analysis, (value) => {
  if (value.kind === "valid") {
    if (!fileNameCustomized.value) fileName.value = value.request.fileName;
    if (!categoryCustomized.value) category.value = categoryForFile(value.request.fileName);
  }
});

watch(category, (value) => {
  if (!destinationCustomized.value)
    destination.value = destinationForCategory(props.settings, value);
});

function customizeDestination(event: Event): void {
  destination.value = (event.target as HTMLInputElement).value;
  destinationCustomized.value = true;
}

function customizeFileName(event: Event): void {
  fileName.value = (event.target as HTMLInputElement).value;
  fileNameCustomized.value = true;
}

function customizeCategory(event: Event): void {
  category.value = (event.target as HTMLSelectElement).value as DownloadCategory;
  categoryCustomized.value = true;
}

function restoreCategoryDestination(): void {
  destinationCustomized.value = false;
  destination.value = destinationForCategory(props.settings, category.value);
}

function submit(): void {
  if (analysis.value.kind !== "valid") {
    submitError.value =
      analysis.value.kind === "invalid"
        ? analysis.value.message
        : "Añade un enlace o comando cURL.";
    return;
  }
  try {
    const request = analysis.value.request;
    const additional = normalizeHeaders(parseHeaderLines(extraHeaders.value));
    const headers = [...request.source.headers, ...additional.headers];
    const cookies = [
      ...request.source.cookies,
      ...additional.cookies,
      ...parseCookies(extraCookies.value),
    ];
    emit("submit", {
      source: {
        ...request.source,
        forceSingleStream: singleStream.value,
        headers,
        cookies,
        proxy:
          proxyId.value === "direct"
            ? { kind: "direct" }
            : { kind: "profile", profileId: proxyId.value },
      },
      fileName: fileName.value || request.fileName,
      fileNameCustomized: fileNameCustomized.value && fileName.value.trim().length > 0,
      category: category.value,
      categoryCustomized: categoryCustomized.value,
      destination: destination.value,
      destinationCustomized: destinationCustomized.value,
      threads: threads.value,
      speedLimitBytes: speedLimitBytes.value,
      startImmediately: startImmediately.value,
    });
    submitError.value = "";
  } catch (error) {
    submitError.value = error instanceof Error ? error.message : "Revisa los campos avanzados.";
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="dialog-backdrop ui-dialog-backdrop"
      role="presentation"
      @mousedown.self="emit('close')"
    >
      <section
        class="dialog-card add-dialog ui-dialog-card max-w-2xl"
        role="dialog"
        aria-modal="true"
        aria-labelledby="new-download-title"
      >
        <header class="ui-dialog-header">
          <div>
            <p class="ui-eyebrow">NUEVA TRANSFERENCIA</p>
            <h2 class="ui-dialog-title" id="new-download-title">Añadir descarga</h2>
          </div>
          <button class="ui-icon-button" type="button" aria-label="Cerrar" @click="emit('close')">
            <AppIcon name="close" />
          </button>
        </header>
        <form class="flex min-h-0 flex-1 flex-col overflow-hidden" @submit.prevent="submit">
          <div class="dialog-body min-h-0 flex-1 overflow-y-auto px-6 py-5">
            <label class="request-field"
              ><span class="ui-label">ENLACE O COMANDO CURL</span
              ><textarea
                class="ui-input min-h-24 w-full resize-y py-3 font-mono text-sm"
                v-model="rawRequest"
                name="request"
                autofocus
                placeholder="Pega https://... o curl 'https://...' -H 'Authorization: ...'"
                spellcheck="false"
              />
            </label>

            <div
              v-if="analysis.kind === 'valid'"
              class="request-preview mt-2.5 flex items-center gap-2.5 rounded-lg border border-line bg-[rgba(194,255,91,.025)] p-2.5"
            >
              <span
                class="preview-icon grid h-8 w-8 place-items-center rounded-lg bg-surface text-accent"
                ><AppIcon name="link" :size="18"
              /></span>
              <div class="min-w-0 flex-1">
                <strong
                  class="block overflow-hidden text-caption text-ellipsis whitespace-nowrap"
                  >{{ analysis.request.fileName }}</strong
                >
                <p class="[margin:3px_0_0] font-mono text-muted">
                  {{ hostOf(analysis.request.source.url) }} ·
                  {{ analysis.request.source.headers.length }} headers ·
                  {{ analysis.request.source.cookies.length }} cookies
                </p>
              </div>
              <span class="valid-mark text-accent"><AppIcon name="check" :size="15" /></span>
            </div>
            <p
              v-else-if="analysis.kind === 'invalid'"
              class="field-error [margin:9px_0_0] flex items-center gap-1.5 text-danger"
            >
              <AppIcon name="warning" :size="14" />{{ analysis.message }}
            </p>
            <div
              v-if="analysis.kind === 'valid' && analysis.request.warnings.length"
              class="warnings mt-2 border-l-2 border-l-warning bg-[rgba(211,162,77,.04)] px-2.5 py-[7px]"
            >
              <p
                class="mx-0 my-0.5 text-warning"
                v-for="warning in analysis.request.warnings"
                :key="warning"
              >
                {{ warning }}
              </p>
            </div>

            <div
              class="quick-grid mt-[17px] grid grid-cols-[minmax(0,2fr)_minmax(0,1fr)] gap-3 max-dialog:grid-cols-[1fr]"
            >
              <label
                ><span class="ui-label">ARCHIVO</span
                ><input
                  class="ui-input w-full"
                  :value="fileName"
                  name="file-name"
                  placeholder="Se detecta automáticamente"
                  @input="customizeFileName"
              /></label>
              <label
                ><span class="ui-label">CATEGORÍA</span
                ><select
                  class="ui-input w-full"
                  :value="category"
                  name="category"
                  @change="customizeCategory"
                >
                  <option v-for="item in categories" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select></label
              >
            </div>

            <button
              class="advanced-toggle mt-3.5 flex h-[37px] w-full cursor-pointer items-center gap-[7px] border-0 border-t border-b border-t-line border-b-line bg-transparent px-1 py-0 text-xs text-muted"
              type="button"
              :aria-expanded="advanced"
              @click="advanced = !advanced"
            >
              <AppIcon name="settings" :size="15" />Configuración avanzada<AppIcon
                class="[&.app-icon:last-child]:ml-auto [&.app-icon:last-child]:[transition:transform_.2s] [&.rotated]:rotate-90"
                name="chevron"
                :size="14"
                :class="{ rotated: advanced }"
              />
            </button>
            <div
              v-if="advanced"
              class="advanced-fields mt-3.5 grid grid-cols-[1fr_1fr] gap-3 max-dialog:grid-cols-[1fr]"
            >
              <label
                class="[&:nth-child(n+3)]:[grid-column:1/-1] max-dialog:[&:nth-child(n+3)]:[grid-column:auto]"
                ><span class="ui-label">MÁX. SEGMENTOS</span
                ><select class="ui-input w-full" v-model.number="threads" name="threads">
                  <option
                    v-for="value in [1, 2, 4, 6, 8, 12, 16, 24, 32]"
                    :key="value"
                    :value="value"
                  >
                    Hasta {{ value }}
                  </option>
                </select></label
              >
              <label
                class="[&:nth-child(n+3)]:[grid-column:1/-1] max-dialog:[&:nth-child(n+3)]:[grid-column:auto]"
                ><span class="ui-label">CONEXIONES</span
                ><select class="ui-input w-full" v-model="singleStream" name="single-stream">
                  <option :value="false">Automáticas según servidor</option>
                  <option :value="true">Una solicitud continua</option>
                </select></label
              >
              <p class="segment-note m-0 [align-self:center] text-xs leading-[1.5] text-muted">
                Se usarán solo las conexiones que admita el servidor. La reanudación se comprueba al
                conectar.
              </p>
              <label
                class="destination-field [&:nth-child(n+3)]:[grid-column:1/-1] max-dialog:[&:nth-child(n+3)]:[grid-column:auto]"
                ><span class="ui-label"
                  >DESTINO
                  <button
                    class="[float:right] cursor-pointer border-0 bg-transparent p-0 font-mono text-accent"
                    v-if="destinationCustomized"
                    type="button"
                    @click="restoreCategoryDestination"
                  >
                    Usar categoría
                  </button></span
                >
                <div
                  class="input-icon relative [&>.app-icon]:absolute [&>.app-icon]:top-[11px] [&>.app-icon]:left-[9px] [&>.app-icon]:text-muted"
                >
                  <AppIcon name="folder" :size="15" /><input
                    class="ui-input w-full"
                    :value="destination"
                    name="destination"
                    @input="customizeDestination"
                  />
                </div>
                <small class="mt-[5px] block font-mono text-muted">{{
                  destinationPreview
                }}</small></label
              >
              <label
                class="[&:nth-child(n+3)]:[grid-column:1/-1] max-dialog:[&:nth-child(n+3)]:[grid-column:auto]"
                ><span class="ui-label">PERFIL DE RED</span
                ><select class="ui-input w-full" v-model="proxyId" name="proxy">
                  <option value="direct">Conexión directa</option>
                  <option
                    v-for="proxy in proxies"
                    :key="proxy.id"
                    :value="proxy.id"
                    :disabled="!proxy.enabled"
                  >
                    {{ proxy.name }}{{ proxy.enabled ? "" : " (desactivado)" }}
                  </option></select
                ><small class="mt-[5px] block text-muted" v-if="!nativeRuntime"
                  >Los proxies solo se aplican en la aplicación de escritorio.</small
                ></label
              >
              <div class="speed-limit-field [grid-column:1/-1]">
                <span class="mb-[7px] block font-mono tracking-[.11em] text-muted"
                  >LÍMITE DE VELOCIDAD</span
                ><SpeedLimitInput v-model="speedLimitBytes" name="speed-limit" /><small
                  class="mt-[5px] block font-mono text-muted"
                  >Se aplica al total de todos los segmentos.</small
                >
              </div>
              <label
                class="[&:nth-child(n+3)]:[grid-column:1/-1] max-dialog:[&:nth-child(n+3)]:[grid-column:auto]"
                ><span class="ui-label"
                  >HEADERS ADICIONALES
                  <i class="[float:right] text-muted not-italic">uno por línea</i></span
                ><textarea
                  v-model="extraHeaders"
                  name="extra-headers"
                  class="ui-input compact min-h-24 w-full resize-y py-3 font-mono text-sm"
                  placeholder="Referer: https://example.com&#10;Authorization: Bearer ..."
                />
              </label>
              <div
                v-if="additionalHeaderWarnings.length"
                class="warnings [grid-column:1/-1] mt-0 border-l-2 border-l-warning bg-[rgba(211,162,77,.04)] px-2.5 py-[7px]"
              >
                <p
                  class="mx-0 my-0.5 text-warning"
                  v-for="warning in additionalHeaderWarnings"
                  :key="warning"
                >
                  {{ warning }}
                </p>
              </div>
              <label
                class="[&:nth-child(n+3)]:[grid-column:1/-1] max-dialog:[&:nth-child(n+3)]:[grid-column:auto]"
                ><span class="ui-label">COOKIES ADICIONALES</span
                ><textarea
                  v-model="extraCookies"
                  name="extra-cookies"
                  class="ui-input compact min-h-24 w-full resize-y py-3 font-mono text-sm"
                  placeholder="session=abc123; preference=dark"
                />
              </label>
            </div>
            <label class="start-option mt-4 flex cursor-pointer items-center gap-2.5 px-0 py-2.5"
              ><input
                class="pointer-events-none absolute h-[37px] w-full rounded-lg border border-line bg-canvas px-2.5 py-0 font-sans text-sm text-foreground opacity-0 outline-none"
                v-model="startImmediately"
                name="start-immediately"
                type="checkbox"
              /><i
                class="h-[17px] w-7.5 rounded-xl bg-line p-0.5 after:block after:h-[13px] after:w-[13px] after:rounded-full after:bg-muted after:content-[''] after:[transition:.2s] [.start-option_input:checked+&]:bg-accent [.start-option_input:checked+&::after]:[transform:translateX(13px)] [.start-option_input:checked+&::after]:bg-surface"
              ></i>
              <div>
                <strong class="block text-xs">Iniciar al añadir</strong
                ><small class="mt-0.5 block text-muted"
                  >Respeta el límite global de descargas simultáneas.</small
                >
              </div></label
            >
            <p
              v-if="submitError"
              class="field-error [margin:9px_0_0] flex items-center gap-1.5 text-danger"
            >
              <AppIcon name="warning" :size="14" />{{ submitError }}
            </p>
          </div>
          <footer class="ui-dialog-footer flex-wrap">
            <p class="m-0 flex items-center gap-[5px] text-muted max-dialog:hidden">
              <AppIcon name="shield" :size="14" />Las credenciales no aparecen en eventos ni
              errores.
            </p>
            <div class="flex gap-[7px]">
              <button class="ui-button" type="button" @click="emit('close')">Cancelar</button
              ><button
                class="submit-button ui-button ui-button-primary"
                type="submit"
                :disabled="busy || analysis.kind !== 'valid'"
              >
                <AppIcon name="download" :size="16" />{{ busy ? "Añadiendo…" : "Añadir descarga" }}
              </button>
            </div>
          </footer>
        </form>
      </section>
    </div>
  </Teleport>
</template>
