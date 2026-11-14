<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { DownloadAction, DownloadItem, DownloadSource } from "@/domain/download";
import { parseRequest } from "@/domain/ingest";
import { formatBytes, formatEta, formatSpeed, hostOf, redactUrl } from "@/domain/format";
import { progressOf, recoveryAction } from "@/domain/download";
import AppIcon from "./AppIcon.vue";
import SegmentMonitor from "./SegmentMonitor.vue";
import SpeedLimitInput from "./SpeedLimitInput.vue";

const props = defineProps<{ item: DownloadItem; canReveal: boolean }>();
const emit = defineEmits<{
  close: [];
  action: [action: DownloadAction];
  reveal: [];
  replace: [source: DownloadSource];
  setSpeedLimit: [bytes: number];
}>();
const replacing = ref(false);
const replacement = ref("");
const replacementError = ref("");
const speedLimitDraft = ref(props.item.speedLimitBytes);
const speedLimitApplied = ref(false);

watch(
  () => props.item.id,
  () => {
    replacing.value = false;
    replacement.value = "";
    replacementError.value = "";
    speedLimitDraft.value = props.item.speedLimitBytes;
    speedLimitApplied.value = false;
  },
  { immediate: true },
);

const total = computed(() =>
  props.item.transfer.size.kind === "known" ? props.item.transfer.size.totalBytes : undefined,
);
const speed = computed(() =>
  props.item.state.kind === "downloading" ? props.item.state.speedBytes : 0,
);
const progressKnown = computed(() => props.item.transfer.size.kind === "known");
const progressIndeterminate = computed(
  () => !progressKnown.value && props.item.state.kind === "downloading",
);
const visualProgress = computed(() =>
  progressKnown.value ? progressOf(props.item) : props.item.state.kind === "completed" ? 100 : 0,
);
const progressLabel = computed(() => {
  if (progressKnown.value) return `${progressOf(props.item)}%`;
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
const stateLabel = computed(
  () =>
    ({
      queued: "En cola",
      downloading: "Descargando",
      paused: "Pausada",
      completed: "Completada",
      failed: "Error",
    })[props.item.state.kind],
);
const validatorLabel = computed(() => {
  if (props.item.transfer.validator.kind === "none") return "Sin validador";
  return props.item.transfer.validator.kind === "etag"
    ? "ETag verificado"
    : "Last-Modified verificado";
});
const resumeMeta = computed(() => {
  switch (props.item.transfer.resume.kind) {
    case "supported":
      return {
        label: "Reanudable",
        detail:
          props.item.transfer.validator.kind === "none"
            ? "Rangos confirmados; sin validador de identidad"
            : "Rangos y validador confirmados",
        className: "supported",
      };
    case "unsupported":
      return {
        label: "No reanudable",
        detail: props.item.transfer.resume.reason,
        className: "unsupported",
      };
    case "unknown":
      return {
        label: "Por comprobar",
        detail: "Se determinará al conectar con el servidor",
        className: "unknown",
      };
  }
});
watch(
  () => [props.item.id, props.item.speedLimitBytes] as const,
  ([id, value], [previousId]) => {
    speedLimitDraft.value = value;
    if (id !== previousId) return;
    speedLimitApplied.value = true;
    window.setTimeout(() => {
      speedLimitApplied.value = false;
    }, 1800);
  },
);

const failedAction = computed(() => recoveryAction(props.item));
const pausedAction = computed(() => recoveryAction(props.item));

function submitReplacement(): void {
  try {
    const parsed = parseRequest(replacement.value);
    emit("replace", {
      ...parsed.source,
      proxy: props.item.source.proxy,
      forceSingleStream: props.item.source.forceSingleStream,
    });
    replacing.value = false;
    replacementError.value = "";
  } catch (error) {
    replacementError.value = error instanceof Error ? error.message : "Enlace inválido";
  }
}

function applySpeedLimit(): void {
  if (speedLimitDraft.value === props.item.speedLimitBytes) return;
  emit("setSpeedLimit", speedLimitDraft.value);
}
</script>

<template>
  <aside
    class="detail-panel sticky top-0 h-screen w-83.5 shrink-0 grow-0 basis-83.5 overflow-hidden border-l border-l-line bg-canvas max-inspector:fixed max-inspector:top-0 max-inspector:right-0 max-inspector:z-40 max-inspector:shadow-[-20px_0_50px_rgba(0,0,0,.4)] max-narrow:w-full"
  >
    <header class="flex h-20 items-center justify-between border-b border-b-line px-5 py-0">
      <div>
        <p class="[margin:0_0_4px] font-mono tracking-[.14em] text-muted">INSPECTOR</p>
        <h2 class="m-0 text-sm">Detalle de descarga</h2>
      </div>
      <button class="ui-icon-button" type="button" aria-label="Cerrar" @click="emit('close')">
        <AppIcon name="close" />
      </button>
    </header>
    <div class="detail-scroll h-[calc(100vh_-_80px)] overflow-auto [padding:22px_20px_40px]">
      <div class="file-hero text-center">
        <span class="m-auto grid h-[57px] w-13 place-items-center rounded-xl bg-elevated text-muted"
          ><AppIcon name="file" :size="26"
        /></span>
        <h3 class="[margin:14px_auto_5px] max-w-65 text-xs wrap-anywhere">
          {{ item.fileName }}
        </h3>
        <p class="m-0 font-mono text-muted">{{ hostOf(item.source.url) }}</p>
      </div>
      <div
        class="state-banner mt-4.5 rounded-lg border border-line bg-raised px-2.5 py-[9px] [&.completed]:border-line [&.downloading]:border-line [&.failed]:border-danger/40 [&.section-note]:leading-[1.55] [&.section-note]:text-muted [&.segments]:grid [&.segments]:grid-cols-[repeat(8,1fr)] [&.segments]:gap-[3px]"
        :class="item.state.kind"
      >
        <span
          class="font-mono tracking-[.08em] text-foreground uppercase [.state-banner.completed_&]:text-success [.state-banner.downloading_&]:text-accent [.state-banner.failed_&]:text-danger"
          >{{ stateLabel }}</span
        >
        <p
          class="[margin:4px_0_0] leading-[1.5] text-muted [.state-banner.failed_&]:text-danger"
          v-if="item.state.kind === 'failed'"
        >
          {{ item.state.message }}
        </p>
        <p
          class="[margin:4px_0_0] leading-[1.5] text-muted [.state-banner.failed_&]:text-danger"
          v-else
        >
          {{ item.telemetry.phase === "idle" ? resumeMeta.detail : "" }}
        </p>
      </div>
      <div
        class="hero-progress mt-5.5 [&.section-note]:leading-[1.55] [&.section-note]:text-muted [&.segments]:grid [&.segments]:grid-cols-[repeat(8,1fr)] [&.segments]:gap-[3px]"
        :class="{ indeterminate: progressIndeterminate }"
      >
        <div class="first:flex first:items-end first:justify-between">
          <strong
            class="font-mono text-[22px] [.hero-progress.indeterminate_&]:text-[15px] [.hero-progress.indeterminate_&]:text-foreground"
            >{{ progressLabel }}</strong><span class="font-mono text-muted"
            >{{ formatBytes(item.transfer.downloadedBytes)
            }}<template v-if="progressKnown"> / {{ formatBytes(total) }}</template></span
          >
        </div>
        <div
          class="track mt-[9px] h-1 overflow-hidden rounded-md bg-line first:flex first:items-end first:justify-between"
          :role="progressKnown ? 'progressbar' : undefined"
          :aria-label="progressKnown ? `Progreso de ${item.fileName}` : undefined"
          :aria-valuemin="progressKnown ? 0 : undefined"
          :aria-valuemax="progressKnown ? 100 : undefined"
          :aria-valuenow="progressKnown ? progressOf(item) : undefined"
        >
          <i
            class="block h-full bg-accent [.hero-progress.indeterminate_.track_&]:w-[38%]! [.hero-progress.indeterminate_.track_&]:animate-[fluxor-indeterminate_1.2s_ease-in-out_infinite] [.hero-progress.indeterminate_.track_&]:bg-[linear-gradient(90deg,transparent,var(--color-accent),transparent)] [.segments_&]:h-4.5 [.segments_&]:rounded-md [.segments_&]:bg-line"
            :style="{ width: `${visualProgress}%` }"
          ></i>
        </div>
      </div>
      <div class="detail-actions mt-[17px] grid grid-cols-[1fr] gap-[7px]">
        <button
          v-if="item.state.kind === 'completed' && canReveal"
          class="primary flex h-9 min-h-10 cursor-pointer items-center justify-center gap-1.5 rounded-lg border border-[transparent] bg-accent text-xs font-bold text-surface disabled:border-line disabled:bg-surface disabled:text-muted disabled:opacity-72"
          type="button"
          @click="emit('reveal')"
        >
          <AppIcon name="folder" :size="16" />Mostrar en carpeta
        </button>
        <button
          v-if="item.state.kind === 'downloading'"
          class="primary flex h-9 min-h-10 cursor-pointer items-center justify-center gap-1.5 rounded-lg border border-[transparent] bg-accent text-xs font-bold text-surface disabled:border-line disabled:bg-surface disabled:text-muted disabled:opacity-72"
          type="button"
          :disabled="controlsLocked"
          @click="emit('action', 'pause')"
        >
          <AppIcon :name="controlsLocked ? 'activity' : 'pause'" :size="16" />{{
            controlsLocked ? "Finalizando…" : "Pausar"
          }}
        </button>
        <button
          v-else-if="item.state.kind !== 'completed' && item.state.kind !== 'queued'"
          class="primary flex h-9 min-h-10 cursor-pointer items-center justify-center gap-1.5 rounded-lg border border-[transparent] bg-accent text-xs font-bold text-surface disabled:border-line disabled:bg-surface disabled:text-muted disabled:opacity-72"
          type="button"
          @click="emit('action', item.state.kind === 'failed' ? failedAction : pausedAction)"
        >
          <AppIcon
            :name="item.state.kind === 'failed' || pausedAction === 'restart' ? 'refresh' : 'play'"
            :size="16"
          />{{
            item.state.kind === "failed"
              ? failedAction === "restart"
                ? "Reiniciar"
                : "Reintentar"
              : pausedAction === "restart"
                ? "Reiniciar"
                : "Reanudar"
          }}
        </button>
        <button
          v-if="item.state.kind === 'queued'"
          class="primary flex h-9 min-h-10 cursor-pointer items-center justify-center gap-1.5 rounded-lg border border-[transparent] bg-accent text-xs font-bold text-surface disabled:border-line disabled:bg-surface disabled:text-muted disabled:opacity-72"
          type="button"
          @click="emit('action', 'pause')"
        >
          Pausar cola
        </button>
        <button
          class="flex h-9 min-h-10 cursor-pointer items-center justify-center gap-1.5 rounded-lg border border-line bg-raised text-xs text-muted disabled:border-line disabled:bg-surface disabled:text-muted disabled:opacity-72"
          v-if="item.state.kind !== 'completed'"
          type="button"
          :disabled="controlsLocked"
          @click="replacing = !replacing"
        >
          <AppIcon name="link" :size="16" />Actualizar enlace
        </button>
      </div>

      <form
        v-if="replacing"
        class="replace-form mt-3 rounded-lg border border-line bg-raised p-3"
        @submit.prevent="submitReplacement"
      >
        <label class="font-mono tracking-[.08em] text-muted"
          >NUEVO ENLACE O CURL<textarea
            class="ui-input min-h-24 w-full resize-y py-3 font-mono text-sm"
            v-model="replacement"
            name="replacement-request"
            autofocus
            placeholder="https://... o curl 'https://...'"
          />
        </label>
        <p class="mx-0 my-1.5 text-danger" v-if="replacementError">
          {{ replacementError }}
        </p>
        <div class="mt-2 flex justify-end gap-1.5">
          <button
            class="h-[27px] cursor-pointer border-0 bg-transparent text-muted"
            type="button"
            @click="replacing = false"
          >
            Cancelar</button
          ><button
            class="save h-[27px] cursor-pointer rounded-md border-0 bg-accent px-[9px] py-0 text-surface"
            type="submit"
          >
            Validar y reanudar
          </button>
        </div>
      </form>

      <section
        v-if="item.state.kind !== 'completed'"
        class="speed-limit-card mt-3 rounded-lg border border-line bg-raised p-3"
      >
        <h4 class="[margin:0_0_9px] font-mono tracking-[.1em] text-muted">LÍMITE DE VELOCIDAD</h4>
        <SpeedLimitInput
          :key="item.id"
          v-model="speedLimitDraft"
          name="detail-speed-limit"
          label="Límite de velocidad de esta descarga"
        />
        <div class="speed-limit-actions mt-3 flex flex-col items-stretch gap-2">
          <button
            class="ui-button w-full text-sm"
            type="button"
            :disabled="controlsLocked"
            @click="applySpeedLimit"
          >
            <AppIcon name="bolt" :size="13" />{{
              speedLimitApplied ? "Aplicado" : "Aplicar límite"
            }}</button
          ><small class="text-xs leading-relaxed text-muted"
            >Se aplica al instante sobre los segmentos activos.</small
          >
        </div>
      </section>

      <section class="details-section mt-[27px]">
        <h4 class="[margin:0_0_12px] font-mono tracking-[.14em] text-muted">TRANSFERENCIA</h4>
        <dl class="m-0">
          <div
            class="flex min-h-[31px] items-center justify-between gap-[15px] border-b border-b-surface"
          >
            <dt class="text-xs text-muted">Velocidad</dt>
            <dd class="m-0 max-w-47.5 text-right font-mono text-xs text-muted">
              {{ formatSpeed(speed) }}
            </dd>
          </div>
          <div
            class="flex min-h-[31px] items-center justify-between gap-[15px] border-b border-b-surface"
          >
            <dt class="text-xs text-muted">Tiempo restante</dt>
            <dd class="m-0 max-w-47.5 text-right font-mono text-xs text-muted">
              {{ formatEta(total, item.transfer.downloadedBytes, speed) }}
            </dd>
          </div>
          <div
            class="flex min-h-[31px] items-center justify-between gap-[15px] border-b border-b-surface"
          >
            <dt class="text-xs text-muted">Tope</dt>
            <dd class="m-0 max-w-47.5 text-right font-mono text-xs text-muted">
              {{ item.speedLimitBytes > 0 ? formatSpeed(item.speedLimitBytes) : "Sin límite" }}
            </dd>
          </div>
          <div
            class="flex min-h-[31px] items-center justify-between gap-[15px] border-b border-b-surface"
          >
            <dt class="text-xs text-muted">Configurados</dt>
            <dd class="m-0 max-w-47.5 text-right font-mono text-xs text-muted">
              Hasta {{ item.threads }}
            </dd>
          </div>
          <div
            class="flex min-h-[31px] items-center justify-between gap-[15px] border-b border-b-surface"
          >
            <dt class="text-xs text-muted">Reanudación</dt>
            <dd
              class="m-0 max-w-47.5 text-right font-mono text-xs text-muted"
              :title="resumeMeta.detail"
            >
              {{ resumeMeta.label }}
            </dd>
          </div>
          <div
            class="flex min-h-[31px] items-center justify-between gap-[15px] border-b border-b-surface"
          >
            <dt class="text-xs text-muted">Identidad</dt>
            <dd class="m-0 max-w-47.5 text-right font-mono text-xs text-muted">
              {{ validatorLabel }}
            </dd>
          </div>
          <div
            class="flex min-h-[31px] items-center justify-between gap-[15px] border-b border-b-surface"
          >
            <dt class="text-xs text-muted">Destino</dt>
            <dd class="m-0 max-w-47.5 text-right font-mono text-xs text-muted">
              {{ item.destination }}
            </dd>
          </div>
        </dl>
      </section>
      <SegmentMonitor :item="item" />
      <section class="details-section mt-[27px]">
        <h4 class="[margin:0_0_12px] font-mono tracking-[.14em] text-muted">SOLICITUD</h4>
        <dl class="m-0">
          <div
            class="flex min-h-[31px] items-center justify-between gap-[15px] border-b border-b-surface"
          >
            <dt class="text-xs text-muted">Headers</dt>
            <dd class="m-0 max-w-47.5 text-right font-mono text-xs text-muted">
              {{ item.source.headers.length }}
            </dd>
          </div>
          <div
            class="flex min-h-[31px] items-center justify-between gap-[15px] border-b border-b-surface"
          >
            <dt class="text-xs text-muted">Cookies</dt>
            <dd class="m-0 max-w-47.5 text-right font-mono text-xs text-muted">
              {{ item.source.cookies.length }}
            </dd>
          </div>
          <div
            class="flex min-h-[31px] items-center justify-between gap-[15px] border-b border-b-surface"
          >
            <dt class="text-xs text-muted">Ruta</dt>
            <dd
              class="url m-0 max-w-47.5 overflow-hidden text-right font-mono text-xs text-ellipsis whitespace-nowrap text-muted"
              :title="redactUrl(item.source.url)"
            >
              {{ redactUrl(item.source.url) }}
            </dd>
          </div>
        </dl>
      </section>
      <div
        class="integrity mt-6.5 flex gap-2.5 rounded-lg border border-line bg-[rgba(194,255,91,.025)] p-3 text-accent [&.section-note]:leading-[1.55] [&.section-note]:text-muted [&.segments]:grid [&.segments]:grid-cols-[repeat(8,1fr)] [&.segments]:gap-[3px]"
        :class="resumeMeta.className"
      >
        <AppIcon name="shield" :size="18" />
        <div>
          <strong class="block text-foreground">{{ resumeMeta.label }}</strong>
          <p class="[margin:3px_0_0] leading-[1.5] text-muted">
            {{ resumeMeta.detail }}
          </p>
        </div>
      </div>
    </div>
  </aside>
</template>
