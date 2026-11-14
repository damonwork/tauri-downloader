<script setup lang="ts">
import type { DownloadStats } from "@/domain/download";
import { formatSpeed } from "@/domain/format";
import AppIcon from "./AppIcon.vue";

defineProps<{ stats: DownloadStats; maxConcurrent: number }>();
</script>

<template>
  <section
    class="summary-strip [margin:22px_30px_0] flex min-h-19 items-stretch overflow-hidden rounded-lg border border-line bg-[linear-gradient(105deg,#111815,#0e1412)] max-[920px]:overflow-x-auto max-mobile:[margin:14px_16px_0] max-mobile:min-h-[67px] @max-[920px]/workspace:overflow-x-auto @max-[620px]/workspace:mr-4 @max-[620px]/workspace:ml-4 @max-[620px]/workspace:min-h-[67px]"
    aria-label="Resumen de actividad"
  >
    <div
      class="speed-block flex min-w-[265px] items-center gap-4 border-r border-r-line px-6 py-0 max-[920px]:min-w-55 max-mobile:min-w-47.5 max-mobile:px-4 max-mobile:py-0 @max-[920px]/workspace:min-w-55 @max-[620px]/workspace:min-w-47.5 @max-[620px]/workspace:px-4 @max-[620px]/workspace:py-0"
    >
      <span class="pulse flex h-[31px] w-11 items-center gap-[3px]"
        ><i
          class="w-[3px] animate-[fluxor-pulse_1.4s_ease-in-out_infinite] rounded-md bg-accent [&:nth-child(1)]:h-2 [&:nth-child(1)]:opacity-35 [&:nth-child(2)]:h-[19px] [&:nth-child(2)]:opacity-65 [&:nth-child(2)]:[animation-delay:-.2s] [&:nth-child(3)]:h-7.5 [&:nth-child(4)]:h-[19px] [&:nth-child(4)]:opacity-65 [&:nth-child(4)]:[animation-delay:-.4s] [&:nth-child(5)]:h-2 [&:nth-child(5)]:opacity-35"
        ></i
        ><i
          class="w-[3px] animate-[fluxor-pulse_1.4s_ease-in-out_infinite] rounded-md bg-accent [&:nth-child(1)]:h-2 [&:nth-child(1)]:opacity-35 [&:nth-child(2)]:h-[19px] [&:nth-child(2)]:opacity-65 [&:nth-child(2)]:[animation-delay:-.2s] [&:nth-child(3)]:h-7.5 [&:nth-child(4)]:h-[19px] [&:nth-child(4)]:opacity-65 [&:nth-child(4)]:[animation-delay:-.4s] [&:nth-child(5)]:h-2 [&:nth-child(5)]:opacity-35"
        ></i
        ><i
          class="w-[3px] animate-[fluxor-pulse_1.4s_ease-in-out_infinite] rounded-md bg-accent [&:nth-child(1)]:h-2 [&:nth-child(1)]:opacity-35 [&:nth-child(2)]:h-[19px] [&:nth-child(2)]:opacity-65 [&:nth-child(2)]:[animation-delay:-.2s] [&:nth-child(3)]:h-7.5 [&:nth-child(4)]:h-[19px] [&:nth-child(4)]:opacity-65 [&:nth-child(4)]:[animation-delay:-.4s] [&:nth-child(5)]:h-2 [&:nth-child(5)]:opacity-35"
        ></i
        ><i
          class="w-[3px] animate-[fluxor-pulse_1.4s_ease-in-out_infinite] rounded-md bg-accent [&:nth-child(1)]:h-2 [&:nth-child(1)]:opacity-35 [&:nth-child(2)]:h-[19px] [&:nth-child(2)]:opacity-65 [&:nth-child(2)]:[animation-delay:-.2s] [&:nth-child(3)]:h-7.5 [&:nth-child(4)]:h-[19px] [&:nth-child(4)]:opacity-65 [&:nth-child(4)]:[animation-delay:-.4s] [&:nth-child(5)]:h-2 [&:nth-child(5)]:opacity-35"
        ></i
        ><i
          class="w-[3px] animate-[fluxor-pulse_1.4s_ease-in-out_infinite] rounded-md bg-accent [&:nth-child(1)]:h-2 [&:nth-child(1)]:opacity-35 [&:nth-child(2)]:h-[19px] [&:nth-child(2)]:opacity-65 [&:nth-child(2)]:[animation-delay:-.2s] [&:nth-child(3)]:h-7.5 [&:nth-child(4)]:h-[19px] [&:nth-child(4)]:opacity-65 [&:nth-child(4)]:[animation-delay:-.4s] [&:nth-child(5)]:h-2 [&:nth-child(5)]:opacity-35"
        ></i
      ></span>
      <div>
        <small class="mb-1 block font-mono tracking-[.12em] text-muted">VELOCIDAD TOTAL</small
        ><strong
          class="font-mono text-[21px] tracking-[-.04em] max-mobile:text-lg @max-[620px]/workspace:text-lg"
          >{{ formatSpeed(stats.speedBytes) }}</strong
        >
      </div>
    </div>
    <div
      class="summary-metric flex min-w-[145px] items-center gap-[11px] px-6 py-0 max-[920px]:min-w-[125px] max-[920px]:px-4 max-[920px]:py-0 max-mobile:min-w-[105px] @max-[920px]/workspace:min-w-[125px] @max-[920px]/workspace:px-4 @max-[920px]/workspace:py-0 @max-[620px]/workspace:min-w-[105px]"
    >
      <span
        class="metric-icon active grid h-8 w-8 place-items-center rounded-lg bg-[rgba(194,255,91,.09)] text-accent max-mobile:hidden @max-[620px]/workspace:hidden"
        ><AppIcon name="activity" :size="16"
      /></span>
      <div>
        <strong class="block font-mono text-lg"
          >{{ stats.active }}<small class="text-muted">/{{ maxConcurrent }}</small></strong
        >
        <p class="[margin:2px_0_0] text-muted">Activas</p>
      </div>
    </div>
    <div
      class="summary-metric flex min-w-[145px] items-center gap-[11px] px-6 py-0 max-[920px]:min-w-[125px] max-[920px]:px-4 max-[920px]:py-0 max-mobile:min-w-[105px] @max-[920px]/workspace:min-w-[125px] @max-[920px]/workspace:px-4 @max-[920px]/workspace:py-0 @max-[620px]/workspace:min-w-[105px]"
    >
      <span
        class="metric-icon grid h-8 w-8 place-items-center rounded-lg bg-elevated text-muted max-mobile:hidden @max-[620px]/workspace:hidden"
        ><AppIcon name="clock" :size="16"
      /></span>
      <div>
        <strong class="block font-mono text-lg">{{ stats.queued }}</strong>
        <p class="[margin:2px_0_0] text-muted">En cola</p>
      </div>
    </div>
    <div
      class="summary-metric flex min-w-[145px] items-center gap-[11px] px-6 py-0 max-[920px]:min-w-[125px] max-[920px]:px-4 max-[920px]:py-0 max-mobile:min-w-[105px] @max-[920px]/workspace:min-w-[125px] @max-[920px]/workspace:px-4 @max-[920px]/workspace:py-0 @max-[620px]/workspace:min-w-[105px]"
    >
      <span
        class="metric-icon grid h-8 w-8 place-items-center rounded-lg bg-elevated text-muted max-mobile:hidden @max-[620px]/workspace:hidden"
        ><AppIcon name="check" :size="16"
      /></span>
      <div>
        <strong class="block font-mono text-lg">{{ stats.completed }}</strong>
        <p class="[margin:2px_0_0] text-muted">Completadas</p>
      </div>
    </div>
    <div
      v-if="stats.failed"
      class="summary-metric flex min-w-[145px] items-center gap-[11px] px-6 py-0 max-[920px]:min-w-[125px] max-[920px]:px-4 max-[920px]:py-0 max-mobile:min-w-[105px] @max-[920px]/workspace:min-w-[125px] @max-[920px]/workspace:px-4 @max-[920px]/workspace:py-0 @max-[620px]/workspace:min-w-[105px]"
    >
      <span
        class="metric-icon danger grid h-8 w-8 place-items-center rounded-lg bg-elevated text-danger max-mobile:hidden @max-[620px]/workspace:hidden"
        ><AppIcon name="warning" :size="16"
      /></span>
      <div>
        <strong class="block font-mono text-lg">{{ stats.failed }}</strong>
        <p class="[margin:2px_0_0] text-muted">Con error</p>
      </div>
    </div>
  </section>
</template>
