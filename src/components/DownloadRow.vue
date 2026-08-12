<script setup lang="ts">
import { computed } from "vue";
import type { DownloadAction, DownloadItem } from "@/domain/download";
import { progressOf } from "@/domain/download";
import { formatBytes, formatEta, formatSpeed, hostOf, relativeTime } from "@/domain/format";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{ item: DownloadItem; selected: boolean; canReveal: boolean }>();
const emit = defineEmits<{ select: []; action: [action: DownloadAction]; reveal: [] }>();

const progress = computed(() => progressOf(props.item));
const total = computed(() => props.item.transfer.size.kind === "known" ? props.item.transfer.size.totalBytes : undefined);
const progressKnown = computed(() => props.item.transfer.size.kind === "known");
const progressIndeterminate = computed(() => !progressKnown.value && props.item.state.kind === "downloading");
const visualProgress = computed(() => progressKnown.value ? progress.value : props.item.state.kind === "completed" ? 100 : 0);
const progressLabel = computed(() => {
  if (progressKnown.value) return `${progress.value}%`;
  if (props.item.state.kind === "downloading") return "En curso";
  if (props.item.state.kind === "completed") return "Completa";
  if (props.item.state.kind === "queued") return "Pendiente";
  return "Sin tamaño";
});
const controlsLocked = computed(() =>
  props.item.state.kind === "downloading" && ["merging", "finalizing"].includes(props.item.telemetry.phase),
);
const speed = computed(() => props.item.state.kind === "downloading" ? props.item.state.speedBytes : 0);
const activePhaseLabel = computed(() => ({
  idle: "Preparando",
  preparing: "Preparando",
  probing: "Consultando",
  connecting: "Conectando",
  transferring: "Descargando",
  merging: "Combinando",
  finalizing: "Finalizando",
})[props.item.telemetry.phase]);
const transferDetail = computed(() => {
  if (props.item.state.kind !== "downloading") return `Máx. ${props.item.threads} segmentos`;
  if (props.item.telemetry.mode.kind === "single") return "Flujo único";
  const segments = props.item.telemetry.segments;
  if (!segments.length) return activePhaseLabel.value;
  const active = segments.filter((segment) => segment.state === "downloading").length;
  const connecting = segments.filter((segment) => segment.state === "connecting").length;
  if (active > 0) return `${active}/${segments.length} segmentos activos`;
  if (connecting > 0) return `${connecting}/${segments.length} conectando`;
  return `${segments.length} segmentos reales`;
});
const resumeMeta = computed(() => {
  switch (props.item.transfer.resume.kind) {
    case "supported": return {
      label: "Reanudable",
      className: "supported",
      title: props.item.transfer.validator.kind === "none"
        ? "Admite rangos; el servidor no proporciona validador de identidad"
        : "Admite rangos y validación de identidad",
    };
    case "unsupported": return { label: "No reanudable", className: "unsupported", title: props.item.transfer.resume.reason };
    case "unknown": return { label: "Reanudación pendiente", className: "unknown", title: "Se comprobará al conectar" };
  }
});

const stateMeta = computed(() => {
  switch (props.item.state.kind) {
    case "downloading": return { label: activePhaseLabel.value, className: "active" };
    case "queued": return { label: "En cola", className: "queued" };
    case "paused": return { label: "Pausada", className: "paused" };
    case "completed": return { label: "Completada", className: "completed" };
    case "failed": return { label: "Error", className: "failed" };
  }
});

const primaryAction = computed<{ action: DownloadAction; icon: string; label: string }>(() => {
  if (props.item.state.kind === "downloading" || props.item.state.kind === "queued") {
    return { action: "pause", icon: "pause", label: "Pausar" };
  }
  if (props.item.state.kind === "failed") {
    return props.item.state.recoverable
      ? { action: "retry", icon: "refresh", label: "Reintentar" }
      : { action: "restart", icon: "refresh", label: "Reiniciar desde cero" };
  }
  if (props.item.transfer.downloadedBytes > 0 && props.item.transfer.resume.kind === "unsupported") {
    return { action: "restart", icon: "refresh", label: "Reiniciar desde cero" };
  }
  return { action: "resume", icon: "play", label: "Reanudar" };
});
</script>

<template>
  <article class="download-row" :class="{ selected }" tabindex="0" @click="emit('select')" @keydown.enter="emit('select')">
    <div class="file-cell">
      <span class="file-icon" :class="item.category"><AppIcon name="file" :size="20" /></span>
      <div class="file-copy">
        <strong :title="item.fileName">{{ item.fileName }}</strong>
        <p><span>{{ hostOf(item.source.url) }}</span><i></i>{{ relativeTime(item.createdAt) }}<b class="compact-state" :class="stateMeta.className">{{ stateMeta.label }}</b></p>
      </div>
    </div>

    <div class="status-cell">
      <span class="state-chip" :class="stateMeta.className"><i></i>{{ stateMeta.label }}</span>
      <small v-if="item.state.kind === 'failed'" :title="item.state.message">{{ item.state.message }}</small>
      <small v-else class="resume-chip" :class="resumeMeta.className" :title="resumeMeta.title">{{ resumeMeta.label }}</small>
    </div>

    <div class="progress-cell">
      <div class="progress-head"><span>{{ formatBytes(item.transfer.downloadedBytes) }} <i v-if="progressKnown">/ {{ formatBytes(total) }}</i></span><strong>{{ progressLabel }}</strong></div>
      <div class="track" :class="[stateMeta.className,{indeterminate:progressIndeterminate}]" :role="progressKnown ? 'progressbar' : undefined" :aria-label="progressKnown ? `Progreso de ${item.fileName}` : undefined" :aria-valuemin="progressKnown ? 0 : undefined" :aria-valuemax="progressKnown ? 100 : undefined" :aria-valuenow="progressKnown ? progress : undefined"><span :style="{ width: `${visualProgress}%` }"></span></div>
    </div>

    <div class="transfer-cell">
      <strong>{{ formatSpeed(speed) }}</strong>
      <small>{{ item.state.kind === 'downloading' && progressKnown ? `${formatEta(total, item.transfer.downloadedBytes, speed)} · ${transferDetail}` : transferDetail }}</small>
    </div>

    <div class="row-actions" @click.stop>
      <button v-if="item.state.kind !== 'completed'" type="button" :disabled="controlsLocked" :aria-label="controlsLocked ? 'Finalizando archivo' : primaryAction.label" :title="controlsLocked ? 'La finalización no se puede interrumpir' : primaryAction.label" @click="emit('action', primaryAction.action)"><AppIcon :name="controlsLocked ? 'activity' : primaryAction.icon" :size="17" /></button>
      <button v-else-if="canReveal" class="reveal-action" type="button" aria-label="Mostrar en carpeta" title="Mostrar en carpeta" @click="emit('reveal')"><AppIcon name="folder" :size="16" /></button>
      <button class="delete-action" type="button" aria-label="Eliminar" title="Eliminar" @click="emit('action', 'remove')"><AppIcon name="trash" :size="16" /></button>
      <button class="details-action" type="button" aria-label="Ver detalles" title="Ver detalles" @click="emit('select')"><AppIcon name="chevron" :size="16" /></button>
    </div>
  </article>
</template>

<style scoped>
.download-row { min-height: 79px; display: grid; grid-template-columns: minmax(255px,1.35fr) 115px minmax(210px,1fr) 125px 104px; gap: 18px; align-items: center; padding: 11px 15px; border-bottom: 1px solid #1c2421; outline: 0; cursor: pointer; transition: background .16s, box-shadow .16s; }.download-row:hover,.download-row.selected { background: #121915; }.download-row.selected { box-shadow: inset 2px 0 var(--accent); }
.file-cell { min-width: 0; display: flex; align-items: center; gap: 12px; }.file-icon { width: 38px; height: 42px; border-radius: 6px; background: #1a211f; color: #8a9690; display: grid; place-items: center; flex: 0 0 auto; }.file-icon.video { color:#de9d68;background:rgba(222,157,104,.09) }.file-icon.archive { color:#a99fea;background:rgba(169,159,234,.09) }.file-icon.document { color:#67b5d0;background:rgba(103,181,208,.09) }.file-icon.audio { color:#e58fad;background:rgba(229,143,173,.09) }
.file-copy { min-width: 0; }.file-copy strong { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; font-weight: 610; color: #d8dfdb; }.file-copy p { margin: 5px 0 0; color: #7c8781; font: 9px var(--mono); display: flex; align-items: center; gap: 7px; }.file-copy p span { max-width: 130px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.file-copy p i { width: 2px; height: 2px; border-radius:50%;background:#66716b; }.compact-state{display:none;color:#838e88;font:7px var(--mono);font-weight:500;text-transform:uppercase}.compact-state.active{color:var(--accent)}.compact-state.failed{color:var(--danger)}.compact-state.completed{color:#65cda5}
.state-chip { display: inline-flex; align-items: center; gap: 6px; color: #8a938f; font-size: 9px; text-transform: uppercase; letter-spacing: .05em; }.state-chip i { width: 6px; height: 6px; border-radius: 50%; background: #69726e; }.state-chip.active { color: var(--accent); }.state-chip.active i { background: var(--accent); box-shadow:0 0 8px rgba(194,255,91,.4) }.state-chip.completed { color:#65cda5 }.state-chip.completed i { background:#65cda5 }.state-chip.failed { color:var(--danger) }.state-chip.failed i { background:var(--danger) }.state-chip.queued i { border:1px solid #77827c;background:transparent }.status-cell small { display:block;max-width:110px;margin-top:5px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:#a97070;font-size:8px }.status-cell .resume-chip.supported{color:#65cda5}.status-cell .resume-chip.unknown{color:#717c76}.status-cell .resume-chip.unsupported{color:#d3a24d}
.progress-head { display:flex;justify-content:space-between;margin-bottom:7px;font:9px var(--mono);color:#a8b1ad }.progress-head i { color:#7b8580;font-style:normal }.progress-head strong { color:#929d97;font-weight:500 }.track { height:3px;border-radius:3px;background:#252d2a;overflow:hidden }.track span { height:100%;display:block;background:#75817b;border-radius:3px;transition:width .4s ease }.track.active span { background:var(--accent);box-shadow:0 0 9px rgba(194,255,91,.28) }.track.completed span { background:#65cda5 }.track.failed span { background:var(--danger) }.track.indeterminate span{width:34%!important;background:linear-gradient(90deg,transparent,var(--accent),transparent);animation:indeterminate 1.2s ease-in-out infinite}@keyframes indeterminate{from{transform:translateX(-110%)}to{transform:translateX(300%)}}
.transfer-cell strong,.transfer-cell small { display:block }.transfer-cell strong { font:11px var(--mono);color:#c0c8c4 }.transfer-cell small { margin-top:5px;color:#7f8a84;font-size:9px }.row-actions { display:flex;justify-content:flex-end;gap:4px }.row-actions button { width:29px;height:29px;border:1px solid transparent;border-radius:5px;background:transparent;color:#65706b;display:grid;place-items:center;cursor:pointer }.row-actions button:hover { border-color:#313b36;background:#19201d;color:var(--accent) }.row-actions button:disabled{color:#49534e;background:transparent;border-color:transparent}
@media (max-width: 1100px) { .download-row { grid-template-columns:minmax(250px,1.3fr) minmax(180px,1fr) 105px 68px }.status-cell { display:none }.reveal-action{display:none!important} }
@media (max-width:760px) { .download-row { min-height:108px;grid-template-columns:1fr auto;grid-template-rows:auto auto;padding:14px 12px;gap:12px }.file-cell{grid-column:1}.progress-cell{grid-column:1/-1;grid-row:2}.transfer-cell{display:none}.row-actions{grid-column:2;grid-row:1}.delete-action{display:grid}.details-action{display:none} }
@container workspace (max-width:1100px){.download-row{grid-template-columns:minmax(180px,1.3fr) minmax(130px,1fr) 90px 68px;gap:12px}.status-cell{display:none}.compact-state{display:inline}.reveal-action{display:none!important}.details-action{display:none}}
@container workspace (max-width:650px){.download-row{min-height:108px;grid-template-columns:1fr auto;grid-template-rows:auto auto;padding:14px 12px;gap:12px}.file-cell{grid-column:1}.progress-cell{grid-column:1/-1;grid-row:2}.transfer-cell{display:none}.row-actions{grid-column:2;grid-row:1}.row-actions button:nth-child(2){display:grid}.row-actions button:last-child{display:none}}
</style>
