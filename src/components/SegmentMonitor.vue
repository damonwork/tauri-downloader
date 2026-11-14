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
  return {
    idle: "Preparando",
    preparing: "Preparando destino",
    probing: "Consultando servidor",
    connecting: "Abriendo conexiones",
    transferring: "Transfiriendo datos",
    merging: "Combinando segmentos",
    finalizing: "Finalizando archivo",
  }[props.item.telemetry.phase];
});

const modeLabel = computed(() => {
  const mode = props.item.telemetry.mode;
  if (mode.kind === "segmented")
    return `${props.item.telemetry.segments.length} segmentos efectivos`;
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
  const order: SegmentState[] = [
    "downloading",
    "connecting",
    "completed",
    "paused",
    "pending",
    "failed",
    "stopped",
  ];
  const counts = new Map<SegmentState, number>();
  props.item.telemetry.segments.forEach((segment) =>
    counts.set(segment.state, (counts.get(segment.state) ?? 0) + 1),
  );
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
  if (waitingForSegments.value)
    return "El mapa aparecerá cuando el motor determine el modo efectivo.";
  if (props.item.state.kind === "completed")
    return "Esta descarga se completó antes de registrar telemetría por segmento.";
  if (props.item.state.kind === "failed")
    return "No se alcanzó a registrar un mapa de segmentos para esta ejecución.";
  return "El modo efectivo se determinará cuando la descarga conecte con el servidor.";
});

watch(
  () => [props.item.id, props.item.telemetry.segments.map((segment) => segment.index).join(",")],
  () => {
    if (props.item.telemetry.segments.some((segment) => segment.index === selectedIndex.value))
      return;
    selectedIndex.value =
      props.item.telemetry.segments.find((segment) =>
        ["failed", "downloading", "connecting"].includes(segment.state),
      )?.index ?? props.item.telemetry.segments[0]?.index;
  },
  { immediate: true },
);

function stateMeta(state: SegmentState): { label: string; short: string } {
  return {
    pending: { label: "En espera", short: "Espera" },
    connecting: { label: "Abriendo conexión", short: "Conecta" },
    downloading: { label: "Descargando", short: "Activo" },
    paused: { label: "Pausado", short: "Pausa" },
    completed: { label: "Completado", short: "Listo" },
    failed: { label: "Fallido", short: "Error" },
    stopped: { label: "Detenido", short: "Detenido" },
  }[state];
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
  const seconds = Math.max(
    0,
    Math.floor((Date.now() - new Date(segment.lastActivityAt).getTime()) / 1000),
  );
  if (seconds < 2) return "ahora";
  if (seconds < 60) return `hace ${seconds} s`;
  return `hace ${Math.floor(seconds / 60)} min`;
}
</script>

<template>
  <section
    class="segment-monitor mt-[25px] border-t border-t-line pt-4.5"
    aria-labelledby="segment-title"
  >
    <div class="segment-heading flex items-start justify-between gap-3">
      <div>
        <h4 class="[margin:0_0_7px] font-mono tracking-[.13em] text-muted" id="segment-title">
          SEGMENTOS
        </h4>
        <strong class="text-foreground">{{ modeLabel }}</strong>
      </div>
      <span
        class="phase max-w-31.5 rounded-lg border border-line bg-surface px-[7px] py-[5px] text-right font-mono tracking-[.03em] text-muted [&.connecting]:border-muted [&.connecting]:text-warning [&.finalizing]:border-line [&.finalizing]:text-info [&.merging]:border-line [&.merging]:text-info [&.probing]:border-muted [&.probing]:text-warning [&.transferring]:border-success/40 [&.transferring]:bg-[rgba(194,255,91,.035)] [&.transferring]:text-accent"
        :class="item.telemetry.phase"
        >{{ phaseLabel }}</span
      >
    </div>
    <p class="mode-detail [margin:8px_0_0] leading-[1.5] text-muted">
      {{ modeDetail }}
    </p>
    <p v-if="stateSummary" class="state-summary [margin:11px_0_0] font-mono text-muted">
      {{ stateSummary }}
    </p>

    <div
      v-if="item.telemetry.segments.length"
      class="segment-grid mt-3 grid max-h-53.5 grid-cols-[repeat(4,minmax(0,1fr))] gap-1.5 overflow-auto pr-0.5 max-narrow:grid-cols-[repeat(4,minmax(56px,1fr))]"
    >
      <button
        v-for="segment in item.telemetry.segments"
        :key="segment.index"
        type="button"
        class="segment-tile relative [isolation:isolate] h-12 min-w-0 cursor-pointer overflow-hidden rounded-lg border border-line bg-raised [padding:7px_7px_6px] text-left text-muted hover:border-line [&.completed]:border-line [&.connecting]:border-muted [&.connecting::after]:absolute [&.connecting::after]:inset-0 [&.connecting::after]:-z-1 [&.connecting::after]:animate-[fluxor-connection_1.2s_linear_infinite] [&.connecting::after]:bg-[repeating-linear-gradient(115deg,transparent_0_8px,rgba(207,168,85,.08)_8px_15px)] [&.connecting::after]:content-[''] [&.failed]:border-danger/40 [&.selected]:border-success/40 [&.selected]:shadow-[0_0_0_1px_rgba(194,255,91,.12)] [&.stopped]:opacity-68"
        :class="[
          segment.state,
          { selected: selectedIndex === segment.index, indeterminate: segment.endByte === null },
        ]"
        :title="segmentLabel(segment)"
        :aria-pressed="selectedIndex === segment.index"
        @click="selectedIndex = segment.index"
      >
        <span
          class="segment-fill absolute [inset:0_auto_0_0] -z-1 border-r border-r-[rgba(194,255,91,.38)] bg-[linear-gradient(90deg,rgba(194,255,91,.08),rgba(194,255,91,.17))] [transition:width_.35s_ease] [.segment-tile.completed_&]:w-full! [.segment-tile.completed_&]:border-success [.segment-tile.completed_&]:bg-[rgba(101,205,165,.14)] [.segment-tile.failed_&]:border-danger [.segment-tile.failed_&]:bg-[rgba(220,119,112,.12)] [.segment-tile.indeterminate_&]:hidden"
          :style="{ width: `${progress(segment)}%` }"
        ></span>
        <span class="segment-copy flex justify-between gap-[3px]"
          ><b class="font-mono text-foreground">S{{ segment.index + 1 }}</b
          ><em class="font-mono text-muted not-italic">{{
            segmentProgressOf(segment) === undefined ? "—" : `${progress(segment)}%`
          }}</em></span
        >
        <small
          class="mt-[9px] block overflow-hidden font-mono text-ellipsis whitespace-nowrap text-muted [.segment-tile.completed_&]:text-success [.segment-tile.connecting_&]:text-warning [.segment-tile.downloading_&]:text-accent [.segment-tile.failed_&]:text-danger [.segment-tile.paused_&]:text-warning"
          >{{ stateMeta(segment.state).short }}</small
        >
      </button>
    </div>
    <div
      v-else
      class="segments-pending mt-3 flex min-h-[67px] items-center gap-2.5 rounded-lg border border-dashed border-line p-[13px]"
      :class="{ active: waitingForSegments }"
    >
      <i
        class="h-5 w-5 animate-[fluxor-spin_.9s_linear_infinite] rounded-full border-2 border-line [border-top-color:#9dad69]"
        v-if="waitingForSegments"
      ></i>
      <p class="m-0 leading-[1.5] text-muted">{{ emptyMessage }}</p>
    </div>

    <div v-if="selected" class="segment-detail mt-2.5 rounded-lg border border-line bg-surface p-3">
      <header class="flex items-start justify-between">
        <div>
          <span class="block font-mono tracking-[.1em] text-muted"
            >SEGMENTO {{ selected.index + 1 }}</span
          ><strong
            class="mt-1 block text-foreground [&.completed]:text-success [&.connecting]:text-warning [&.downloading]:text-accent [&.failed]:text-danger"
            :class="selected.state"
            >{{ stateMeta(selected.state).label }}</strong
          >
        </div>
        <b class="font-mono text-base text-foreground">{{
          segmentProgressOf(selected) === undefined ? "—" : `${progress(selected)}%`
        }}</b>
      </header>
      <dl class="[margin:14px_0_0] grid grid-cols-[1fr_1fr] gap-[10px_12px]">
        <div class="min-w-0">
          <dt class="font-mono text-muted uppercase">Descargado</dt>
          <dd
            class="[margin:4px_0_0] overflow-hidden font-mono text-ellipsis whitespace-nowrap text-muted"
          >
            {{ formatBytes(selected.downloadedBytes) }} / {{ formatBytes(segmentSize(selected)) }}
          </dd>
        </div>
        <div class="min-w-0">
          <dt class="font-mono text-muted uppercase">Velocidad</dt>
          <dd
            class="[margin:4px_0_0] overflow-hidden font-mono text-ellipsis whitespace-nowrap text-muted"
          >
            {{ formatSpeed(selected.speedBytes) }}
          </dd>
        </div>
        <div class="min-w-0">
          <dt class="font-mono text-muted uppercase">Estimado</dt>
          <dd
            class="[margin:4px_0_0] overflow-hidden font-mono text-ellipsis whitespace-nowrap text-muted"
          >
            {{ formatEta(segmentSize(selected), selected.downloadedBytes, selected.speedBytes) }}
          </dd>
        </div>
        <div class="min-w-0">
          <dt class="font-mono text-muted uppercase">Actividad</dt>
          <dd
            class="[margin:4px_0_0] overflow-hidden font-mono text-ellipsis whitespace-nowrap text-muted"
          >
            {{ lastActivity(selected) }}
          </dd>
        </div>
        <div class="min-w-0">
          <dt class="font-mono text-muted uppercase">Rango</dt>
          <dd
            class="[margin:4px_0_0] overflow-hidden font-mono text-ellipsis whitespace-nowrap text-muted"
          >
            {{ formatBytes(selected.startByte) }} –
            {{ formatBytes(selected.endByte === null ? undefined : selected.endByte + 1) }}
          </dd>
        </div>
      </dl>
      <p
        v-if="selected.error"
        class="segment-error [margin:11px_0_0] border-t border-t-line pt-2.5 leading-[1.45] text-danger"
      >
        {{ selected.error }}
      </p>
    </div>
  </section>
</template>
