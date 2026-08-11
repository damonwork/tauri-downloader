<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { CreateDownloadInput, DownloadCategory, ParsedRequest } from "@/domain/download";
import { categoryForFile } from "@/domain/download";
import { IngestError, parseCookies, parseHeaderLines, parseRequest } from "@/domain/ingest";
import type { AppSettings, ProxyProfile } from "@/domain/settings";
import { destinationForCategory } from "@/domain/settings";
import { hostOf } from "@/domain/format";
import AppIcon from "./AppIcon.vue";

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
      message: error instanceof IngestError ? error.message : "No se pudo interpretar la solicitud.",
    };
  }
});
const destinationPreview = computed(() => /^[a-zA-Z]:[\\/]|^\//.test(destination.value.trim())
  ? destination.value
  : `Descargas / ${destination.value}`,
);

watch(() => props.open, (open) => {
  if (!open) return;
  rawRequest.value = "";
  fileName.value = "";
  fileNameCustomized.value = false;
  category.value = "other";
  categoryCustomized.value = false;
  destinationCustomized.value = false;
  destination.value = destinationForCategory(props.settings, category.value);
  threads.value = props.settings.defaultThreads;
  startImmediately.value = props.settings.startImmediately;
  proxyId.value = "direct";
  extraHeaders.value = "";
  extraCookies.value = "";
  advanced.value = false;
  submitError.value = "";
});

watch(analysis, (value) => {
  if (value.kind === "valid") {
    if (!fileNameCustomized.value) fileName.value = value.request.fileName;
    if (!categoryCustomized.value) category.value = categoryForFile(value.request.fileName);
  }
});

watch(category, (value) => {
  if (!destinationCustomized.value) destination.value = destinationForCategory(props.settings, value);
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
    submitError.value = analysis.value.kind === "invalid" ? analysis.value.message : "Añade un enlace o comando cURL.";
    return;
  }
  try {
    const request = analysis.value.request;
    const headers = [...request.source.headers, ...parseHeaderLines(extraHeaders.value)];
    const cookies = [...request.source.cookies, ...parseCookies(extraCookies.value)];
    emit("submit", {
      source: {
        ...request.source,
        headers,
        cookies,
        proxy: proxyId.value === "direct"
          ? { kind: "direct" }
          : { kind: "profile", profileId: proxyId.value },
      },
      fileName: fileName.value || request.fileName,
      category: category.value,
      destination: destination.value,
      threads: threads.value,
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
    <div v-if="open" class="dialog-backdrop" role="presentation" @mousedown.self="emit('close')">
      <section class="dialog-card add-dialog" role="dialog" aria-modal="true" aria-labelledby="new-download-title">
        <header><div><p>NUEVA TRANSFERENCIA</p><h2 id="new-download-title">Añadir descarga</h2></div><button type="button" aria-label="Cerrar" @click="emit('close')"><AppIcon name="close" /></button></header>
        <form @submit.prevent="submit">
          <div class="dialog-body">
            <label class="request-field"><span>ENLACE O COMANDO CURL</span><textarea v-model="rawRequest" name="request" autofocus placeholder="Pega https://... o curl 'https://...' -H 'Authorization: ...'" spellcheck="false" /></label>

            <div v-if="analysis.kind === 'valid'" class="request-preview">
              <span class="preview-icon"><AppIcon name="link" :size="18" /></span>
              <div><strong>{{ analysis.request.fileName }}</strong><p>{{ hostOf(analysis.request.source.url) }} · {{ analysis.request.source.headers.length }} headers · {{ analysis.request.source.cookies.length }} cookies</p></div>
              <span class="valid-mark"><AppIcon name="check" :size="15" /></span>
            </div>
            <p v-else-if="analysis.kind === 'invalid'" class="field-error"><AppIcon name="warning" :size="14" />{{ analysis.message }}</p>
            <div v-if="analysis.kind === 'valid' && analysis.request.warnings.length" class="warnings"><p v-for="warning in analysis.request.warnings" :key="warning">{{ warning }}</p></div>

            <div class="quick-grid">
              <label><span>ARCHIVO</span><input :value="fileName" name="file-name" placeholder="Se detecta automáticamente" @input="customizeFileName" /></label>
              <label><span>CATEGORÍA</span><select :value="category" name="category" @change="customizeCategory"><option v-for="item in categories" :key="item.value" :value="item.value">{{ item.label }}</option></select></label>
              <label><span>SEGMENTOS</span><select v-model.number="threads" name="threads"><option v-for="value in [1,2,4,6,8,12,16,24,32]" :key="value" :value="value">{{ value }} hilos</option></select></label>
            </div>

            <button class="advanced-toggle" type="button" @click="advanced=!advanced"><AppIcon name="settings" :size="15" />Configuración avanzada<AppIcon name="chevron" :size="14" :class="{ rotated: advanced }" /></button>
            <div v-if="advanced" class="advanced-fields">
              <label class="destination-field"><span>DESTINO <button v-if="destinationCustomized" type="button" @click="restoreCategoryDestination">Usar categoría</button></span><div class="input-icon"><AppIcon name="folder" :size="15" /><input :value="destination" name="destination" @input="customizeDestination" /></div><small>{{ destinationPreview }}</small></label>
              <label><span>PERFIL DE RED</span><select v-model="proxyId" name="proxy"><option value="direct">Conexión directa</option><option v-for="proxy in proxies" :key="proxy.id" :value="proxy.id" :disabled="!proxy.enabled">{{ proxy.name }}{{ proxy.enabled ? '' : ' (desactivado)' }}</option></select><small v-if="!nativeRuntime">Los proxies solo se aplican en la aplicación de escritorio.</small></label>
              <label><span>HEADERS ADICIONALES <i>uno por línea</i></span><textarea v-model="extraHeaders" name="extra-headers" class="compact" placeholder="Referer: https://example.com&#10;Authorization: Bearer ..." /></label>
              <label><span>COOKIES ADICIONALES</span><textarea v-model="extraCookies" name="extra-cookies" class="compact" placeholder="session=abc123; preference=dark" /></label>
            </div>
            <label class="start-option"><input v-model="startImmediately" name="start-immediately" type="checkbox" /><i></i><div><strong>Iniciar al añadir</strong><small>Respeta el límite global de descargas simultáneas.</small></div></label>
            <p v-if="submitError" class="field-error"><AppIcon name="warning" :size="14" />{{ submitError }}</p>
          </div>
          <footer><p><AppIcon name="shield" :size="14" />Las credenciales no aparecen en eventos ni errores.</p><div><button type="button" @click="emit('close')">Cancelar</button><button class="submit-button" type="submit" :disabled="busy"><AppIcon name="download" :size="16" />{{ busy ? 'Añadiendo…' : 'Añadir descarga' }}</button></div></footer>
        </form>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.dialog-backdrop{position:fixed;inset:0;z-index:100;background:rgba(2,5,4,.76);backdrop-filter:blur(8px);display:grid;place-items:center;padding:20px}.dialog-card{max-height:calc(100vh - 40px);overflow:hidden;border:1px solid #303a35;border-radius:10px;background:#0e1411;box-shadow:0 30px 100px rgba(0,0,0,.55)}.dialog-card>header{height:72px;padding:0 22px;border-bottom:1px solid var(--line);display:flex;align-items:center;justify-content:space-between}.dialog-card header p{margin:0 0 4px;color:#65706a;font:8px var(--mono);letter-spacing:.14em}.dialog-card h2{margin:0;font-size:17px}.dialog-card header button{width:32px;height:32px;border:0;background:transparent;color:#77817c;cursor:pointer}.dialog-card form{display:flex;flex-direction:column;max-height:calc(100vh - 112px)}.dialog-body{padding:20px 22px;overflow:auto}.dialog-body label>span{display:block;margin-bottom:7px;color:#7b8580;font:8px var(--mono);letter-spacing:.11em}.dialog-body label>span i{float:right;font-style:normal;color:#505954}.request-field textarea{width:100%;height:94px;padding:12px;border:1px solid #2c3631;border-radius:7px;resize:none;background:#090e0c;color:#d8dfdb;font:10px/1.6 var(--mono);outline:none}.request-field textarea:focus{border-color:#536348}.request-preview{margin-top:10px;padding:10px;border:1px solid #28352d;border-radius:7px;background:rgba(194,255,91,.025);display:flex;align-items:center;gap:10px}.preview-icon{width:32px;height:32px;border-radius:5px;background:#18201c;color:var(--accent);display:grid;place-items:center}.request-preview div{min-width:0;flex:1}.request-preview strong{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:10px}.request-preview p{margin:3px 0 0;color:#66716b;font:8px var(--mono)}.valid-mark{color:var(--accent)}.field-error{margin:9px 0 0;color:#d98078;font-size:9px;display:flex;align-items:center;gap:6px}.warnings{margin-top:8px;padding:7px 10px;border-left:2px solid #d3a24d;background:rgba(211,162,77,.04)}.warnings p{margin:2px 0;color:#a88651;font-size:8px}.quick-grid,.advanced-fields{display:grid;grid-template-columns:1fr 150px;gap:12px;margin-top:17px}.dialog-body input,.dialog-body select{width:100%;height:37px;padding:0 10px;border:1px solid #29332e;border-radius:6px;outline:0;background:#0a0f0d;color:#bfc8c3;font:10px var(--sans)}.advanced-toggle{width:100%;height:37px;margin-top:14px;padding:0 4px;border:0;border-top:1px solid #1e2722;border-bottom:1px solid #1e2722;background:transparent;color:#7d8882;display:flex;align-items:center;gap:7px;font-size:9px;cursor:pointer}.advanced-toggle .app-icon:last-child{margin-left:auto;transition:transform .2s}.advanced-toggle .rotated{transform:rotate(90deg)}.advanced-fields{grid-template-columns:1fr 1fr;margin-top:14px}.advanced-fields label:nth-child(n+3){grid-column:1/-1}.advanced-fields small{display:block;margin-top:5px;color:#5c6661;font-size:8px}.input-icon{position:relative}.input-icon>.app-icon{position:absolute;left:9px;top:11px;color:#66716b}.input-icon input{padding-left:31px}.dialog-body textarea.compact{width:100%;height:57px;padding:8px;border:1px solid #29332e;border-radius:6px;resize:vertical;background:#0a0f0d;color:#bfc8c3;font:9px/1.5 var(--mono);outline:0}.start-option{margin-top:16px;padding:10px 0;display:flex;align-items:center;gap:10px;cursor:pointer}.start-option>input{position:absolute;opacity:0;pointer-events:none}.start-option>i{width:30px;height:17px;padding:2px;border-radius:10px;background:#29322e}.start-option>i::after{content:"";display:block;width:13px;height:13px;border-radius:50%;background:#78827d;transition:.2s}.start-option input:checked+i{background:var(--accent)}.start-option input:checked+i::after{background:#11170f;transform:translateX(13px)}.start-option strong,.start-option small{display:block}.start-option strong{font-size:9px}.start-option small{margin-top:2px;color:#606a65;font-size:8px}.dialog-card footer{min-height:62px;padding:10px 22px;border-top:1px solid var(--line);display:flex;align-items:center;justify-content:space-between;gap:15px}.dialog-card footer p{margin:0;color:#606b65;font-size:8px;display:flex;align-items:center;gap:5px}.dialog-card footer>div{display:flex;gap:7px}.dialog-card footer button{height:34px;padding:0 12px;border:1px solid #29332e;border-radius:6px;background:#121815;color:#87918c;font-size:9px;cursor:pointer}.dialog-card footer .submit-button{border:0;background:var(--accent);color:#11170f;font-weight:700;display:flex;align-items:center;gap:7px}.dialog-card footer .submit-button:disabled{opacity:.55;cursor:wait}
.dialog-card{width:min(680px,100%)}.quick-grid{grid-template-columns:minmax(0,1fr) 130px 110px}.advanced-fields{grid-template-columns:1fr 1fr}.destination-field>span button{float:right;padding:0;border:0;background:transparent;color:var(--accent);font:8px var(--mono);cursor:pointer}.destination-field>small{display:block;margin-top:5px;color:#5c6761;font:8px var(--mono)}
@media(max-width:600px){.dialog-backdrop{padding:0;align-items:end}.dialog-card{max-height:94vh;border-radius:12px 12px 0 0}.quick-grid,.advanced-fields{grid-template-columns:1fr}.advanced-fields label:nth-child(n+3){grid-column:auto}.dialog-card footer p{display:none}.dialog-card footer{justify-content:flex-end}}
</style>
