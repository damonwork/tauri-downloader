<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { DownloadAction, DownloadItem, DownloadSource } from "@/domain/download";
import { parseRequest } from "@/domain/ingest";
import { formatBytes, formatSpeed, hostOf, redactUrl } from "@/domain/format";
import { progressOf } from "@/domain/download";
import AppIcon from "./AppIcon.vue";
import SegmentMonitor from "./SegmentMonitor.vue";

const props = defineProps<{ item: DownloadItem; canReveal: boolean }>();
const emit = defineEmits<{ close: []; action: [action: DownloadAction]; reveal: []; replace: [source: DownloadSource] }>();
const replacing = ref(false);
const replacement = ref("");
const replacementError = ref("");

watch(() => props.item.id, () => { replacing.value = false; replacement.value = ""; replacementError.value = ""; });

const total = computed(() => props.item.transfer.size.kind === "known" ? props.item.transfer.size.totalBytes : undefined);
const speed = computed(() => props.item.state.kind === "downloading" ? props.item.state.speedBytes : 0);
const progressKnown = computed(() => props.item.transfer.size.kind === "known");
const progressIndeterminate = computed(() => !progressKnown.value && props.item.state.kind === "downloading");
const visualProgress = computed(() => progressKnown.value ? progressOf(props.item) : props.item.state.kind === "completed" ? 100 : 0);
const progressLabel = computed(() => {
  if (progressKnown.value) return `${progressOf(props.item)}%`;
  if (props.item.state.kind === "downloading") return "En curso";
  if (props.item.state.kind === "completed") return "Completa";
  if (props.item.state.kind === "queued") return "Pendiente";
  return "Sin tamaño";
});
const controlsLocked = computed(() =>
  props.item.state.kind === "downloading" && ["merging", "finalizing"].includes(props.item.telemetry.phase),
);
const stateLabel = computed(() => ({
  queued: "En cola",
  downloading: "Descargando",
  paused: "Pausada",
  completed: "Completada",
  failed: "Error",
})[props.item.state.kind]);
const validatorLabel = computed(() => {
  if (props.item.transfer.validator.kind === "none") return "Sin validador";
  return props.item.transfer.validator.kind === "etag" ? "ETag verificado" : "Last-Modified verificado";
});
const resumeMeta = computed(() => {
  switch (props.item.transfer.resume.kind) {
    case "supported": return {
      label: "Reanudable",
      detail: props.item.transfer.validator.kind === "none"
        ? "Rangos confirmados; sin validador de identidad"
        : "Rangos y validador confirmados",
      className: "supported",
    };
    case "unsupported": return { label: "No reanudable", detail: props.item.transfer.resume.reason, className: "unsupported" };
    case "unknown": return { label: "Por comprobar", detail: "Se determinará al conectar con el servidor", className: "unknown" };
  }
});
const failedAction = computed<DownloadAction>(() =>
  props.item.state.kind === "failed" && !props.item.state.recoverable ? "restart" : "retry",
);
const pausedAction = computed<DownloadAction>(() =>
  props.item.transfer.downloadedBytes > 0 && props.item.transfer.resume.kind === "unsupported"
    ? "restart"
    : "resume",
);

function submitReplacement(): void {
  try {
    const parsed = parseRequest(replacement.value);
    emit("replace", { ...parsed.source, proxy: props.item.source.proxy });
    replacing.value = false;
    replacementError.value = "";
  } catch (error) {
    replacementError.value = error instanceof Error ? error.message : "Enlace inválido";
  }
}
</script>

<template>
  <aside class="detail-panel">
    <header><div><p>INSPECTOR</p><h2>Detalle de descarga</h2></div><button type="button" aria-label="Cerrar" @click="emit('close')"><AppIcon name="close" /></button></header>
    <div class="detail-scroll">
      <div class="file-hero"><span><AppIcon name="file" :size="26" /></span><h3>{{ item.fileName }}</h3><p>{{ hostOf(item.source.url) }}</p></div>
      <div class="state-banner" :class="item.state.kind"><span>{{ stateLabel }}</span><p v-if="item.state.kind === 'failed'">{{ item.state.message }}</p><p v-else>{{ item.telemetry.phase === 'idle' ? resumeMeta.detail : '' }}</p></div>
      <div class="hero-progress" :class="{ indeterminate: progressIndeterminate }"><div><strong>{{ progressLabel }}</strong><span>{{ formatBytes(item.transfer.downloadedBytes) }}<template v-if="progressKnown"> / {{ formatBytes(total) }}</template></span></div><div class="track" :role="progressKnown ? 'progressbar' : undefined" :aria-label="progressKnown ? `Progreso de ${item.fileName}` : undefined" :aria-valuemin="progressKnown ? 0 : undefined" :aria-valuemax="progressKnown ? 100 : undefined" :aria-valuenow="progressKnown ? progressOf(item) : undefined"><i :style="{width:`${visualProgress}%`}"></i></div></div>
      <div class="detail-actions">
        <button v-if="item.state.kind === 'completed' && canReveal" class="primary" type="button" @click="emit('reveal')"><AppIcon name="folder" :size="16" />Mostrar en carpeta</button>
        <button v-if="item.state.kind === 'downloading'" class="primary" type="button" :disabled="controlsLocked" @click="emit('action','pause')"><AppIcon :name="controlsLocked ? 'activity' : 'pause'" :size="16" />{{ controlsLocked ? 'Finalizando…' : 'Pausar' }}</button>
        <button v-else-if="item.state.kind !== 'completed'" class="primary" type="button" @click="emit('action',item.state.kind === 'failed' ? failedAction : pausedAction)"><AppIcon :name="item.state.kind === 'failed' || pausedAction === 'restart' ? 'refresh' : 'play'" :size="16" />{{ item.state.kind === 'failed' ? (failedAction === 'restart' ? 'Reiniciar' : 'Reintentar') : (pausedAction === 'restart' ? 'Reiniciar' : 'Reanudar') }}</button>
        <button v-if="item.state.kind !== 'completed'" type="button" :disabled="controlsLocked" @click="replacing = !replacing"><AppIcon name="link" :size="16" />Actualizar enlace</button>
      </div>

      <form v-if="replacing" class="replace-form" @submit.prevent="submitReplacement"><label>NUEVO ENLACE O CURL<textarea v-model="replacement" name="replacement-request" autofocus placeholder="https://... o curl 'https://...'" /></label><p v-if="replacementError">{{ replacementError }}</p><div><button type="button" @click="replacing=false">Cancelar</button><button class="save" type="submit">Validar y reanudar</button></div></form>

      <section class="details-section"><h4>TRANSFERENCIA</h4><dl><div><dt>Velocidad</dt><dd>{{ formatSpeed(speed) }}</dd></div><div><dt>Tope</dt><dd>{{ item.speedLimitBytes > 0 ? formatSpeed(item.speedLimitBytes) : 'Sin límite' }}</dd></div><div><dt>Configurados</dt><dd>Hasta {{ item.threads }}</dd></div><div><dt>Reanudación</dt><dd :title="resumeMeta.detail">{{ resumeMeta.label }}</dd></div><div><dt>Identidad</dt><dd>{{ validatorLabel }}</dd></div><div><dt>Destino</dt><dd>{{ item.destination }}</dd></div></dl></section>
      <SegmentMonitor :item="item" />
      <section class="details-section"><h4>SOLICITUD</h4><dl><div><dt>Headers</dt><dd>{{ item.source.headers.length }}</dd></div><div><dt>Cookies</dt><dd>{{ item.source.cookies.length }}</dd></div><div><dt>Ruta</dt><dd class="url" :title="redactUrl(item.source.url)">{{ redactUrl(item.source.url) }}</dd></div></dl></section>
      <div class="integrity" :class="resumeMeta.className"><AppIcon name="shield" :size="18" /><div><strong>{{ resumeMeta.label }}</strong><p>{{ resumeMeta.detail }}</p></div></div>
    </div>
  </aside>
</template>

<style scoped>
.detail-panel{width:334px;flex:0 0 334px;height:100vh;border-left:1px solid var(--line);background:#0a0f0d;position:sticky;top:0;overflow:hidden}.detail-panel>header{height:80px;padding:0 20px;border-bottom:1px solid var(--line);display:flex;align-items:center;justify-content:space-between}.detail-panel header p{margin:0 0 4px;color:#626d67;font:8px var(--mono);letter-spacing:.14em}.detail-panel h2{margin:0;font-size:14px}.detail-panel header button{width:32px;height:32px;border:0;background:transparent;color:#6b7570;cursor:pointer}.detail-scroll{height:calc(100vh - 80px);overflow:auto;padding:22px 20px 40px}.file-hero{text-align:center}.file-hero>span{width:52px;height:57px;border-radius:9px;background:#171e1b;color:#8f9a94;display:grid;place-items:center;margin:auto}.file-hero h3{margin:14px auto 5px;max-width:260px;font-size:12px;overflow-wrap:anywhere}.file-hero p{margin:0;color:#66706b;font:9px var(--mono)}.hero-progress{margin-top:22px}.hero-progress>div:first-child{display:flex;align-items:flex-end;justify-content:space-between}.hero-progress strong{font:22px var(--mono)}.hero-progress span{color:#6c7671;font:8px var(--mono)}.track{height:4px;margin-top:9px;background:#212925;border-radius:4px;overflow:hidden}.track i{display:block;height:100%;background:var(--accent)}.detail-actions{margin-top:17px;display:grid;grid-template-columns:1fr 1.35fr;gap:7px}.detail-actions button{height:36px;border:1px solid #29322e;border-radius:6px;background:#121815;color:#8f9994;font-size:9px;display:flex;align-items:center;justify-content:center;gap:6px;cursor:pointer}.detail-actions button.primary{border-color:transparent;background:var(--accent);color:#11180e;font-weight:700}.replace-form{margin-top:12px;padding:12px;border:1px solid #334038;border-radius:7px;background:#101613}.replace-form label{color:#68736d;font:8px var(--mono);letter-spacing:.08em}.replace-form textarea{display:block;width:100%;height:68px;margin-top:7px;padding:8px;resize:none;border:1px solid #29332e;border-radius:5px;background:#090e0c;color:#d6ddd9;font:9px/1.5 var(--mono);outline:none}.replace-form p{margin:6px 0;color:var(--danger);font-size:9px}.replace-form>div{margin-top:8px;display:flex;justify-content:flex-end;gap:6px}.replace-form button{height:27px;border:0;background:transparent;color:#7f8984;font-size:9px;cursor:pointer}.replace-form button.save{padding:0 9px;border-radius:4px;background:var(--accent);color:#11170f}.details-section{margin-top:27px}.details-section h4{margin:0 0 12px;color:#69736e;font:8px var(--mono);letter-spacing:.14em}.details-section dl{margin:0}.details-section dl>div{min-height:31px;border-bottom:1px solid #1a211e;display:flex;align-items:center;justify-content:space-between;gap:15px}.details-section dt{color:#68726d;font-size:9px}.details-section dd{margin:0;max-width:190px;color:#aab3ae;font:9px var(--mono);text-align:right}.details-section dd.url{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.segments{display:grid;grid-template-columns:repeat(8,1fr);gap:3px}.segments i{height:18px;border-radius:2px;background:#232c27}.segments i.complete{background:var(--accent)}.section-note{color:#5e6863;font-size:9px;line-height:1.55}.integrity{margin-top:26px;padding:12px;border:1px solid #27322c;border-radius:7px;background:rgba(194,255,91,.025);display:flex;gap:10px;color:var(--accent)}.integrity strong{display:block;font-size:9px;color:#aeb8b2}.integrity p{margin:3px 0 0;color:#65706a;font-size:8px;line-height:1.4}
.state-banner{margin-top:18px;padding:9px 10px;border:1px solid #2d3732;border-radius:6px;background:#111714}.state-banner span{color:#b4beb9;font:8px var(--mono);letter-spacing:.08em;text-transform:uppercase}.state-banner p{margin:4px 0 0;color:#68736d;font-size:8px;line-height:1.4}.state-banner.downloading{border-color:#3d5032}.state-banner.downloading span{color:var(--accent)}.state-banner.failed{border-color:#56332f}.state-banner.failed span,.state-banner.failed p{color:#c67b75}.state-banner.completed{border-color:#2e493d}.state-banner.completed span{color:#65cda5}.hero-progress.indeterminate .track i{width:38%!important;background:linear-gradient(90deg,transparent,var(--accent),transparent);animation:indeterminate 1.2s ease-in-out infinite}.hero-progress.indeterminate strong{font-size:15px;color:#b9c4be}@keyframes indeterminate{from{transform:translateX(-110%)}to{transform:translateX(280%)}}
.detail-panel header p,.file-hero p,.hero-progress span,.details-section h4,.details-section dt{color:#84908a}
.detail-actions button:disabled{border-color:#314038;background:#18201c;color:#77837d;opacity:.72}
@media(max-width:1180px){.detail-panel{position:fixed;z-index:40;right:0;top:0;box-shadow:-20px 0 50px rgba(0,0,0,.4)}}@media(max-width:520px){.detail-panel{width:100%;}}
</style>
