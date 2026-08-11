<script setup lang="ts">
import { computed } from "vue";
import type { DownloadAction, DownloadItem } from "@/domain/download";
import { progressOf } from "@/domain/download";
import { formatBytes, formatEta, formatSpeed, hostOf, relativeTime } from "@/domain/format";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{ item: DownloadItem; selected: boolean }>();
const emit = defineEmits<{ select: []; action: [action: DownloadAction] }>();

const progress = computed(() => progressOf(props.item));
const total = computed(() => props.item.transfer.size.kind === "known" ? props.item.transfer.size.totalBytes : undefined);
const speed = computed(() => props.item.state.kind === "downloading" ? props.item.state.speedBytes : 0);
const resumeMeta = computed(() => {
  switch (props.item.transfer.resume.kind) {
    case "supported": return { label: "Reanudable", className: "supported", title: "Admite rangos y validación segura" };
    case "unsupported": return { label: "No reanudable", className: "unsupported", title: props.item.transfer.resume.reason };
    case "unknown": return { label: "Reanudación pendiente", className: "unknown", title: "Se comprobará al conectar" };
  }
});

const stateMeta = computed(() => {
  switch (props.item.state.kind) {
    case "downloading": return { label: "Descargando", className: "active" };
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
        <p><span>{{ hostOf(item.source.url) }}</span><i></i>{{ relativeTime(item.createdAt) }}</p>
      </div>
    </div>

    <div class="status-cell">
      <span class="state-chip" :class="stateMeta.className"><i></i>{{ stateMeta.label }}</span>
      <small v-if="item.state.kind === 'failed'" :title="item.state.message">{{ item.state.message }}</small>
      <small v-else class="resume-chip" :class="resumeMeta.className" :title="resumeMeta.title">{{ resumeMeta.label }}</small>
    </div>

    <div class="progress-cell">
      <div class="progress-head"><span>{{ formatBytes(item.transfer.downloadedBytes) }} <i>/ {{ formatBytes(total) }}</i></span><strong>{{ progress }}%</strong></div>
      <div class="track" :class="stateMeta.className"><span :style="{ width: `${progress}%` }"></span></div>
    </div>

    <div class="transfer-cell">
      <strong>{{ formatSpeed(speed) }}</strong>
      <small>{{ item.state.kind === 'downloading' ? `${formatEta(total, item.transfer.downloadedBytes, speed)} restantes` : `Máx. ${item.threads} segmentos` }}</small>
    </div>

    <div class="row-actions" @click.stop>
      <button v-if="item.state.kind !== 'completed'" type="button" :aria-label="primaryAction.label" :title="primaryAction.label" @click="emit('action', primaryAction.action)"><AppIcon :name="primaryAction.icon" :size="17" /></button>
      <button type="button" aria-label="Eliminar" title="Eliminar" @click="emit('action', 'remove')"><AppIcon name="trash" :size="16" /></button>
      <button type="button" aria-label="Ver detalles" title="Ver detalles" @click="emit('select')"><AppIcon name="chevron" :size="16" /></button>
    </div>
  </article>
</template>

<style scoped>
.download-row { min-height: 79px; display: grid; grid-template-columns: minmax(255px,1.35fr) 115px minmax(210px,1fr) 125px 104px; gap: 18px; align-items: center; padding: 11px 15px; border-bottom: 1px solid #1c2421; outline: 0; cursor: pointer; transition: background .16s, box-shadow .16s; }.download-row:hover,.download-row.selected { background: #121915; }.download-row.selected { box-shadow: inset 2px 0 var(--accent); }
.file-cell { min-width: 0; display: flex; align-items: center; gap: 12px; }.file-icon { width: 38px; height: 42px; border-radius: 6px; background: #1a211f; color: #8a9690; display: grid; place-items: center; flex: 0 0 auto; }.file-icon.video { color:#de9d68;background:rgba(222,157,104,.09) }.file-icon.archive { color:#a99fea;background:rgba(169,159,234,.09) }.file-icon.document { color:#67b5d0;background:rgba(103,181,208,.09) }.file-icon.audio { color:#e58fad;background:rgba(229,143,173,.09) }
.file-copy { min-width: 0; }.file-copy strong { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; font-weight: 610; color: #d8dfdb; }.file-copy p { margin: 5px 0 0; color: #7c8781; font: 9px var(--mono); display: flex; align-items: center; gap: 7px; }.file-copy p span { max-width: 130px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.file-copy p i { width: 2px; height: 2px; border-radius:50%;background:#66716b; }
.state-chip { display: inline-flex; align-items: center; gap: 6px; color: #8a938f; font-size: 9px; text-transform: uppercase; letter-spacing: .05em; }.state-chip i { width: 6px; height: 6px; border-radius: 50%; background: #69726e; }.state-chip.active { color: var(--accent); }.state-chip.active i { background: var(--accent); box-shadow:0 0 8px rgba(194,255,91,.4) }.state-chip.completed { color:#65cda5 }.state-chip.completed i { background:#65cda5 }.state-chip.failed { color:var(--danger) }.state-chip.failed i { background:var(--danger) }.state-chip.queued i { border:1px solid #77827c;background:transparent }.status-cell small { display:block;max-width:110px;margin-top:5px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:#a97070;font-size:8px }.status-cell .resume-chip.supported{color:#65cda5}.status-cell .resume-chip.unknown{color:#717c76}.status-cell .resume-chip.unsupported{color:#d3a24d}
.progress-head { display:flex;justify-content:space-between;margin-bottom:7px;font:9px var(--mono);color:#a8b1ad }.progress-head i { color:#7b8580;font-style:normal }.progress-head strong { color:#929d97;font-weight:500 }.track { height:3px;border-radius:3px;background:#252d2a;overflow:hidden }.track span { height:100%;display:block;background:#75817b;border-radius:3px;transition:width .4s ease }.track.active span { background:var(--accent);box-shadow:0 0 9px rgba(194,255,91,.28) }.track.completed span { background:#65cda5 }.track.failed span { background:var(--danger) }
.transfer-cell strong,.transfer-cell small { display:block }.transfer-cell strong { font:11px var(--mono);color:#c0c8c4 }.transfer-cell small { margin-top:5px;color:#626c67;font-size:9px }.row-actions { display:flex;justify-content:flex-end;gap:4px }.row-actions button { width:29px;height:29px;border:1px solid transparent;border-radius:5px;background:transparent;color:#65706b;display:grid;place-items:center;cursor:pointer }.row-actions button:hover { border-color:#313b36;background:#19201d;color:var(--accent) }
@media (max-width: 1100px) { .download-row { grid-template-columns:minmax(250px,1.3fr) minmax(180px,1fr) 105px 68px }.status-cell { display:none }.row-actions button:nth-child(2){display:none} }
@media (max-width:760px) { .download-row { min-height:108px;grid-template-columns:1fr auto;grid-template-rows:auto auto;padding:14px 12px;gap:12px }.file-cell{grid-column:1}.progress-cell{grid-column:1/-1;grid-row:2}.transfer-cell{display:none}.row-actions{grid-column:2;grid-row:1}.row-actions button:nth-child(2){display:grid}.row-actions button:last-child{display:none} }
</style>
