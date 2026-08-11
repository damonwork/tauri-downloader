<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { DownloadItem, SegmentProgress, SegmentState } from "@/domain/download";
import { segmentProgressOf, segmentSize } from "@/domain/download";
import { formatBytes, formatEta, formatSpeed } from "@/domain/format";

const props = defineProps<{ item: DownloadItem }>();
const selectedIndex = ref<number>();

const phaseLabel = computed(() => {
  if (props.item.state.kind === "queued") return "En cola";
  if (props.item.state.kind === "paused") return "Pausada";
  if (props.item.state.kind === "completed") return "Completada";
  if (props.item.state.kind === "failed") return "Transferencia interrumpida";
  return ({
    idle: "Preparando",
    preparing: "Preparando destino",
    probing: "Consultando servidor",
    connecting: "Abriendo conexiones",
    transferring: "Transfiriendo datos",
    merging: "Combinando segmentos",
    finalizing: "Finalizando archivo",
  })[props.item.telemetry.phase];
});

const modeLabel = computed(() => {
  const mode = props.item.telemetry.mode;
  if (mode.kind === "segmented") return `${props.item.telemetry.segments.length} segmentos efectivos`;
  if (mode.kind === "single") return "Flujo único";
  return "Modo por determinar";
});

const modeDetail = computed(() => {
  const mode = props.item.telemetry.mode;
  if (mode.kind === "segmented") {
    const count = props.item.telemetry.segments.length;
    return count === props.item.threads
      ? `El motor utiliza los ${count} segmentos configurados.`
      : `El motor utiliza ${count} de hasta ${props.item.threads} segmentos configurados.`;
  }
  if (mode.kind === "single") return mode.reason ?? "El motor seleccionó un único flujo.";
  return `Se comprobará el servidor antes de utilizar hasta ${props.item.threads} segmentos.`;
});

const stateSummary = computed(() => {
  const labels: Record<SegmentState, [string, string]> = {
    pending: ["en espera", "en espera"],
    connecting: ["conectando", "conectando"],
    downloading: ["descargando", "descargando"],
    paused: ["pausado", "pausados"],
    completed: ["completado", "completados"],
    failed: ["fallido", "fallidos"],
    stopped: ["detenido", "detenidos"],
  };
  const order: SegmentState[] = ["downloading", "connecting", "completed", "paused", "pending", "failed", "stopped"];
  const counts = new Map<SegmentState, number>();
  props.item.telemetry.segments.forEach((segment) => counts.set(segment.state, (counts.get(segment.state) ?? 0) + 1));
  return order
    .filter((state) => counts.has(state))
    .map((state) => {
      const count = counts.get(state) ?? 0;
      return `${count} ${labels[state][count === 1 ? 0 : 1]}`;
    })
    .join(" · ");
});

const selected = computed(() =>
  props.item.telemetry.segments.find((segment) => segment.index === selectedIndex.value),
);
const waitingForSegments = computed(() => props.item.state.kind === "downloading");
const emptyMessage = computed(() => {
  if (waitingForSegments.value) return "El mapa aparecerá cuando el motor determine el modo efectivo.";
  if (props.item.state.kind === "completed") return "Esta descarga se completó antes de registrar telemetría por segmento.";
  if (props.item.state.kind === "failed") return "No se alcanzó a registrar un mapa de segmentos para esta ejecución.";
  return "El modo efectivo se determinará cuando la descarga conecte con el servidor.";
});

watch(
  () => [props.item.id, props.item.telemetry.segments.map((segment) => segment.index).join(",")],
  () => {
    if (props.item.telemetry.segments.some((segment) => segment.index === selectedIndex.value)) return;
    selectedIndex.value = props.item.telemetry.segments.find((segment) =>
      ["failed", "downloading", "connecting"].includes(segment.state),
    )?.index ?? props.item.telemetry.segments[0]?.index;
  },
  { immediate: true },
);

function stateMeta(state: SegmentState): { label: string; short: string } {
  return ({
    pending: { label: "En espera", short: "Espera" },
    connecting: { label: "Abriendo conexión", short: "Conecta" },
    downloading: { label: "Descargando", short: "Activo" },
    paused: { label: "Pausado", short: "Pausa" },
    completed: { label: "Completado", short: "Listo" },
    failed: { label: "Fallido", short: "Error" },
    stopped: { label: "Detenido", short: "Detenido" },
  })[state];
}

function progress(segment: SegmentProgress): number {
  return segmentProgressOf(segment) ?? 0;
}

function segmentLabel(segment: SegmentProgress): string {
  const percent = segmentProgressOf(segment);
  return `Segmento ${segment.index + 1}, ${stateMeta(segment.state).label}${percent === undefined ? "" : `, ${percent}%`}`;
}

function lastActivity(segment: SegmentProgress): string {
  if (!segment.lastActivityAt) return "Sin datos recibidos";
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(segment.lastActivityAt).getTime()) / 1000));
  if (seconds < 2) return "ahora";
  if (seconds < 60) return `hace ${seconds} s`;
  return `hace ${Math.floor(seconds / 60)} min`;
}
</script>

<template>
  <section class="segment-monitor" aria-labelledby="segment-title">
    <div class="segment-heading">
      <div><h4 id="segment-title">SEGMENTOS</h4><strong>{{ modeLabel }}</strong></div>
      <span class="phase" :class="item.telemetry.phase">{{ phaseLabel }}</span>
    </div>
    <p class="mode-detail">{{ modeDetail }}</p>
    <p v-if="stateSummary" class="state-summary">{{ stateSummary }}</p>

    <div v-if="item.telemetry.segments.length" class="segment-grid">
      <button
        v-for="segment in item.telemetry.segments"
        :key="segment.index"
        type="button"
        class="segment-tile"
        :class="[segment.state, { selected: selectedIndex === segment.index, indeterminate: segment.endByte === null }]"
        :title="segmentLabel(segment)"
        :aria-pressed="selectedIndex === segment.index"
        @click="selectedIndex = segment.index"
      >
        <span class="segment-fill" :style="{ width: `${progress(segment)}%` }"></span>
        <span class="segment-copy"><b>S{{ segment.index + 1 }}</b><em>{{ segmentProgressOf(segment) === undefined ? "—" : `${progress(segment)}%` }}</em></span>
        <small>{{ stateMeta(segment.state).short }}</small>
      </button>
    </div>
    <div v-else class="segments-pending" :class="{ active: waitingForSegments }"><i v-if="waitingForSegments"></i><p>{{ emptyMessage }}</p></div>

    <div v-if="selected" class="segment-detail">
      <header><div><span>SEGMENTO {{ selected.index + 1 }}</span><strong :class="selected.state">{{ stateMeta(selected.state).label }}</strong></div><b>{{ segmentProgressOf(selected) === undefined ? "—" : `${progress(selected)}%` }}</b></header>
      <dl>
        <div><dt>Descargado</dt><dd>{{ formatBytes(selected.downloadedBytes) }} / {{ formatBytes(segmentSize(selected)) }}</dd></div>
        <div><dt>Velocidad</dt><dd>{{ formatSpeed(selected.speedBytes) }}</dd></div>
        <div><dt>Estimado</dt><dd>{{ formatEta(segmentSize(selected), selected.downloadedBytes, selected.speedBytes) }}</dd></div>
        <div><dt>Actividad</dt><dd>{{ lastActivity(selected) }}</dd></div>
        <div><dt>Rango</dt><dd>{{ formatBytes(selected.startByte) }} – {{ formatBytes(selected.endByte === null ? undefined : selected.endByte + 1) }}</dd></div>
      </dl>
      <p v-if="selected.error" class="segment-error">{{ selected.error }}</p>
    </div>
  </section>
</template>

<style scoped>
.segment-monitor{margin-top:25px;padding-top:18px;border-top:1px solid #1d2521}.segment-heading{display:flex;align-items:flex-start;justify-content:space-between;gap:12px}.segment-heading h4{margin:0 0 7px;color:#7d8882;font:8px var(--mono);letter-spacing:.13em}.segment-heading strong{color:#c9d1cd;font-size:11px}.phase{max-width:126px;padding:5px 7px;border:1px solid #2c3731;border-radius:5px;background:#121915;color:#89948e;text-align:right;font:7px var(--mono);letter-spacing:.03em}.phase.transferring{border-color:#415632;color:var(--accent);background:rgba(194,255,91,.035)}.phase.connecting,.phase.probing{border-color:#4d462e;color:#d0ab5f}.phase.merging,.phase.finalizing{border-color:#304a52;color:#72bfd4}.mode-detail{margin:8px 0 0;color:#7f8a84;font-size:8px;line-height:1.5}.state-summary{margin:11px 0 0;color:#9ca6a1;font:8px var(--mono)}
.segment-grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:6px;max-height:214px;margin-top:12px;padding-right:2px;overflow:auto}.segment-tile{position:relative;min-width:0;height:48px;overflow:hidden;padding:7px 7px 6px;border:1px solid #2a332f;border-radius:6px;background:#111714;color:#8b9690;text-align:left;cursor:pointer;isolation:isolate}.segment-tile:hover{border-color:#465149}.segment-tile.selected{border-color:#819f4e;box-shadow:0 0 0 1px rgba(194,255,91,.12)}.segment-fill{position:absolute;z-index:-1;inset:0 auto 0 0;background:linear-gradient(90deg,rgba(194,255,91,.08),rgba(194,255,91,.17));border-right:1px solid rgba(194,255,91,.38);transition:width .35s ease}.segment-copy{display:flex;justify-content:space-between;gap:3px}.segment-copy b{color:#d0d8d4;font:8px var(--mono)}.segment-copy em{color:#a9b3ae;font:7px var(--mono);font-style:normal}.segment-tile small{display:block;margin-top:9px;overflow:hidden;color:#68736d;font:7px var(--mono);text-overflow:ellipsis;white-space:nowrap}.segment-tile.connecting::after{content:"";position:absolute;z-index:-1;inset:0;background:repeating-linear-gradient(115deg,transparent 0 8px,rgba(207,168,85,.08) 8px 15px);animation:connection 1.2s linear infinite}.segment-tile.connecting{border-color:#4c432b}.segment-tile.connecting small{color:#c9a85e}.segment-tile.downloading small{color:var(--accent)}.segment-tile.completed{border-color:#315244}.segment-tile.completed .segment-fill{width:100%!important;background:rgba(101,205,165,.14);border-color:#65cda5}.segment-tile.completed small{color:#65cda5}.segment-tile.failed{border-color:#683d38}.segment-tile.failed .segment-fill{background:rgba(220,119,112,.12);border-color:var(--danger)}.segment-tile.failed small{color:var(--danger)}.segment-tile.paused small{color:#d3a24d}.segment-tile.stopped{opacity:.68}.segment-tile.indeterminate .segment-fill{display:none}@keyframes connection{to{transform:translateX(15px)}}
.segments-pending{min-height:67px;margin-top:12px;padding:13px;border:1px dashed #29312d;border-radius:6px;display:flex;align-items:center;gap:10px}.segments-pending i{width:20px;height:20px;border:2px solid #303a35;border-top-color:#9dad69;border-radius:50%;animation:spin .9s linear infinite}.segments-pending p{margin:0;color:#65706a;font-size:8px;line-height:1.5}.segment-detail{margin-top:10px;padding:12px;border:1px solid #28322d;border-radius:7px;background:#0e1411}.segment-detail header{display:flex;align-items:flex-start;justify-content:space-between}.segment-detail header span{display:block;color:#5e6963;font:7px var(--mono);letter-spacing:.1em}.segment-detail header strong{display:block;margin-top:4px;color:#c6cec9;font-size:10px}.segment-detail header strong.downloading{color:var(--accent)}.segment-detail header strong.connecting{color:#c9a85e}.segment-detail header strong.completed{color:#65cda5}.segment-detail header strong.failed{color:var(--danger)}.segment-detail header>b{color:#dce4df;font:16px var(--mono)}.segment-detail dl{display:grid;grid-template-columns:1fr 1fr;gap:10px 12px;margin:14px 0 0}.segment-detail dl div{min-width:0}.segment-detail dt{color:#58635d;font:7px var(--mono);text-transform:uppercase}.segment-detail dd{margin:4px 0 0;overflow:hidden;color:#aab4af;font:8px var(--mono);text-overflow:ellipsis;white-space:nowrap}.segment-error{margin:11px 0 0;padding-top:10px;border-top:1px solid #392623;color:#c57c76;font-size:8px;line-height:1.45}
@media(max-width:520px){.segment-grid{grid-template-columns:repeat(4,minmax(56px,1fr))}}
</style>
