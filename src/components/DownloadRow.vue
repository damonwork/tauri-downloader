<script setup lang="ts">
import { computed } from "vue";
import type { DownloadAction, DownloadItem } from "@/domain/download";
import { progressOf, recoveryAction } from "@/domain/download";
import { formatBytes, formatEta, formatSpeed, hostOf, relativeTime } from "@/domain/format";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{ item: DownloadItem; selected: boolean; canReveal: boolean }>();
const emit = defineEmits<{ select: []; action: [action: DownloadAction]; reveal: [] }>();

const progress = computed(() => progressOf(props.item));
const total = computed(() =>
  props.item.transfer.size.kind === "known" ? props.item.transfer.size.totalBytes : undefined,
);
const progressKnown = computed(() => props.item.transfer.size.kind === "known");
const progressIndeterminate = computed(
  () => !progressKnown.value && props.item.state.kind === "downloading",
);
const visualProgress = computed(() =>
  progressKnown.value ? progress.value : props.item.state.kind === "completed" ? 100 : 0,
);
const progressLabel = computed(() => {
  if (progressKnown.value) return `${progress.value}%`;
  if (props.item.state.kind === "downloading") return "En curso";
  if (props.item.state.kind === "completed") return "Completa";
  if (props.item.state.kind === "queued") return "Pendiente";
  return "Sin tamaño";
});
const controlsLocked = computed(
  () =>
    props.item.state.kind === "downloading" &&
    ["merging", "finalizing"].includes(props.item.telemetry.phase),
);
const speed = computed(() =>
  props.item.state.kind === "downloading" ? props.item.state.speedBytes : 0,
);
const activePhaseLabel = computed(
  () =>
    ({
      idle: "Preparando",
      preparing: "Preparando",
      probing: "Consultando",
      connecting: "Conectando",
      transferring: "Descargando",
      merging: "Combinando",
      finalizing: "Finalizando",
    })[props.item.telemetry.phase],
);
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
    case "supported":
      return {
        label: "Reanudable",
        className: "supported",
        title:
          props.item.transfer.validator.kind === "none"
            ? "Admite rangos; el servidor no proporciona validador de identidad"
            : "Admite rangos y validación de identidad",
      };
    case "unsupported":
      return {
        label: "No reanudable",
        className: "unsupported",
        title: props.item.transfer.resume.reason,
      };
    case "unknown":
      return {
        label: "Reanudación pendiente",
        className: "unknown",
        title: "Se comprobará al conectar",
      };
  }
});

const stateMeta = computed(() => {
  switch (props.item.state.kind) {
    case "downloading":
      return { label: activePhaseLabel.value, className: "active" };
    case "queued":
      return { label: "En cola", className: "queued" };
    case "paused":
      return { label: "Pausada", className: "paused" };
    case "completed":
      return { label: "Completada", className: "completed" };
    case "failed":
      return { label: "Error", className: "failed" };
  }
});

const primaryAction = computed<{ action: DownloadAction; icon: string; label: string }>(() => {
  if (props.item.state.kind === "downloading" || props.item.state.kind === "queued") {
    return { action: "pause", icon: "pause", label: "Pausar" };
  }
  const action = recoveryAction(props.item);
  return {
    action,
    icon: action === "resume" ? "play" : "refresh",
    label:
      action === "restart"
        ? "Reiniciar desde cero"
        : action === "retry"
          ? "Reintentar"
          : "Reanudar",
  };
});
</script>

<template>
  <article
    class="download-row grid min-h-[79px] cursor-pointer download-columns items-center gap-4.5 border-b border-b-surface px-[15px] py-[11px] outline-none [transition:background_.16s,_box-shadow_.16s] hover:bg-surface focus-visible:shadow-[inset_0_0_0_2px_var(--color-accent)] max-mobile:min-h-27 max-mobile:grid-rows-[auto_auto] max-mobile:gap-3 max-mobile:px-3 max-mobile:py-3.5 @max-[1100px]/workspace:gap-3 @max-[650px]/workspace:min-h-27 @max-[650px]/workspace:grid-rows-[auto_auto] @max-[650px]/workspace:gap-3 @max-[650px]/workspace:px-3 @max-[650px]/workspace:py-3.5 [&.selected]:bg-surface [&.selected]:shadow-[inset_2px_0_var(--color-accent)]"
    :class="{ selected }"
    tabindex="0"
    @click="emit('select')"
    @keydown.enter="emit('select')"
  >
    <div
      class="file-cell flex min-w-0 items-center gap-3 max-mobile:[grid-column:1] @max-[650px]/workspace:[grid-column:1]"
    >
      <span
        class="file-icon grid h-11 w-10 shrink-0 place-items-center rounded-xl bg-elevated text-muted [&.archive]:bg-info/10 [&.archive]:text-info [&.audio]:bg-audio/10 [&.audio]:text-audio [&.document]:bg-document/10 [&.document]:text-document [&.video]:bg-video/10 [&.video]:text-video"
        :class="item.category"
        ><AppIcon name="file" :size="20"
      /></span>
      <div class="file-copy min-w-0">
        <strong
          class="block overflow-hidden text-sm font-[610] text-ellipsis whitespace-nowrap text-foreground max-mobile:line-clamp-2 max-mobile:whitespace-normal @max-[650px]/workspace:line-clamp-2 @max-[650px]/workspace:whitespace-normal"
          :title="item.fileName"
          >{{ item.fileName }}</strong
        >
        <p class="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 font-mono text-xs text-muted">
          <span class="max-w-32.5 overflow-hidden text-ellipsis whitespace-nowrap">{{
            hostOf(item.source.url)
          }}</span><i class="h-0.5 w-0.5 rounded-full bg-muted"></i>{{ relativeTime(item.createdAt)
          }}<b
            class="compact-state hidden font-mono font-medium text-muted uppercase @max-[1100px]/workspace:inline [&.active]:text-accent [&.completed]:text-success [&.failed]:text-danger"
            :class="stateMeta.className"
            >{{ stateMeta.label }}</b
          >
        </p>
      </div>
    </div>

    <div class="status-cell max-[1100px]:hidden @max-[1100px]/workspace:hidden">
      <span
        class="state-chip inline-flex items-center gap-1.5 tracking-[.05em] text-muted uppercase [&.active]:text-accent [&.completed]:text-success [&.failed]:text-danger"
        :class="stateMeta.className"
        ><i
          class="h-1.5 w-1.5 rounded-full bg-muted [.state-chip.active_&]:bg-accent [.state-chip.active_&]:shadow-[0_0_8px_rgba(194,255,91,.4)] [.state-chip.completed_&]:bg-success [.state-chip.failed_&]:bg-danger [.state-chip.queued_&]:border [.state-chip.queued_&]:border-muted [.state-chip.queued_&]:bg-transparent"
        ></i
        >{{ stateMeta.label }}</span
      >
      <small
        class="mt-[5px] block max-w-27.5 overflow-hidden text-ellipsis whitespace-nowrap text-danger"
        v-if="item.state.kind === 'failed'"
        :title="item.state.message"
        >{{ item.state.message }}</small
      >
      <small
        v-else
        class="resume-chip mt-[5px] block max-w-27.5 overflow-hidden text-ellipsis whitespace-nowrap text-danger [&.supported]:text-success [&.unknown]:text-muted [&.unsupported]:text-warning"
        :class="resumeMeta.className"
        :title="resumeMeta.title"
        >{{ resumeMeta.label }}</small
      >
    </div>

    <div
      class="progress-cell max-mobile:[grid-column:1/-1] max-mobile:[grid-row:2] @max-[650px]/workspace:[grid-column:1/-1] @max-[650px]/workspace:[grid-row:2]"
    >
      <p
        v-if="item.state.kind === 'downloading'"
        class="compact-transfer [margin:0_0_8px] hidden font-mono text-foreground max-mobile:block @max-[650px]/workspace:block"
      >
        {{ formatSpeed(speed) }} · Restante:
        {{ formatEta(total, item.transfer.downloadedBytes, speed) }}
      </p>
      <div class="progress-head mb-[7px] flex justify-between font-mono text-muted">
        <span class="text-xs"
          >{{ formatBytes(item.transfer.downloadedBytes) }}
          <i class="text-muted not-italic" v-if="progressKnown">/ {{ formatBytes(total) }}</i></span
        ><strong class="text-xs font-medium text-muted">{{ progressLabel }}</strong>
      </div>
      <div
        class="track h-[3px] overflow-hidden rounded-md bg-line"
        :class="[stateMeta.className, { indeterminate: progressIndeterminate }]"
        :role="progressKnown ? 'progressbar' : undefined"
        :aria-label="progressKnown ? `Progreso de ${item.fileName}` : undefined"
        :aria-valuemin="progressKnown ? 0 : undefined"
        :aria-valuemax="progressKnown ? 100 : undefined"
        :aria-valuenow="progressKnown ? progress : undefined"
      >
        <span
          class="block h-full rounded-md bg-muted [transition:width_.4s_ease] [.track.active_&]:bg-accent [.track.active_&]:shadow-[0_0_9px_rgba(194,255,91,.28)] [.track.completed_&]:bg-success [.track.failed_&]:bg-danger [.track.indeterminate_&]:w-[34%]! [.track.indeterminate_&]:animate-[fluxor-indeterminate_1.2s_ease-in-out_infinite] [.track.indeterminate_&]:bg-[linear-gradient(90deg,transparent,var(--color-accent),transparent)]"
          :style="{ width: `${visualProgress}%` }"
        ></span>
      </div>
    </div>

    <div class="transfer-cell max-mobile:hidden @max-[650px]/workspace:hidden">
      <strong class="block font-mono text-xs text-foreground">{{ formatSpeed(speed) }}</strong>
      <small class="mt-[5px] block leading-[1.5] text-muted">{{
        item.state.kind === "downloading" && progressKnown
          ? `${formatEta(total, item.transfer.downloadedBytes, speed)} · ${transferDetail}`
          : transferDetail
      }}</small>
    </div>

    <div
      class="row-actions flex justify-end gap-1 max-mobile:[grid-column:2] max-mobile:[grid-row:1] @max-[650px]/workspace:[grid-column:2] @max-[650px]/workspace:[grid-row:1]"
      @click.stop
    >
      <button
        class="ui-icon-button size-9 border-transparent bg-transparent disabled:cursor-not-allowed @max-[650px]/workspace:last:hidden @max-[650px]/workspace:[&:nth-child(2)]:grid"
        v-if="item.state.kind !== 'completed'"
        type="button"
        :disabled="controlsLocked"
        :aria-label="controlsLocked ? 'Finalizando archivo' : primaryAction.label"
        :title="controlsLocked ? 'La finalización no se puede interrumpir' : primaryAction.label"
        @click="emit('action', primaryAction.action)"
      >
        <AppIcon :name="controlsLocked ? 'activity' : primaryAction.icon" :size="17" />
      </button>
      <button
        v-else-if="canReveal"
        class="reveal-action ui-icon-button size-9 border-transparent bg-transparent disabled:cursor-not-allowed @max-[650px]/workspace:last:hidden @max-[650px]/workspace:[&:nth-child(2)]:grid"
        type="button"
        aria-label="Mostrar en carpeta"
        title="Mostrar en carpeta"
        @click="emit('reveal')"
      >
        <AppIcon name="folder" :size="16" />
      </button>
      <button
        class="delete-action ui-icon-button size-9 border-transparent bg-transparent disabled:cursor-not-allowed max-mobile:grid @max-[650px]/workspace:last:hidden @max-[650px]/workspace:[&:nth-child(2)]:grid"
        type="button"
        aria-label="Eliminar"
        title="Eliminar"
        @click="emit('action', 'remove')"
      >
        <AppIcon name="trash" :size="16" />
      </button>
      <button
        class="details-action ui-icon-button size-9 border-transparent bg-transparent disabled:cursor-not-allowed max-mobile:hidden @max-[1100px]/workspace:hidden @max-[650px]/workspace:last:hidden @max-[650px]/workspace:[&:nth-child(2)]:grid"
        type="button"
        aria-label="Ver detalles"
        title="Ver detalles"
        @click="emit('select')"
      >
        <AppIcon name="chevron" :size="16" />
      </button>
    </div>
  </article>
</template>
