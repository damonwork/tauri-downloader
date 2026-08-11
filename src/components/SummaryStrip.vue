<script setup lang="ts">
import type { DownloadStats } from "@/domain/download";
import { formatSpeed } from "@/domain/format";
import AppIcon from "./AppIcon.vue";

defineProps<{ stats: DownloadStats; maxConcurrent: number }>();
</script>

<template>
  <section class="summary-strip" aria-label="Resumen de actividad">
    <div class="speed-block">
      <span class="pulse"><i></i><i></i><i></i><i></i><i></i></span>
      <div><small>VELOCIDAD TOTAL</small><strong>{{ formatSpeed(stats.speedBytes) }}</strong></div>
    </div>
    <div class="summary-metric"><span class="metric-icon active"><AppIcon name="activity" :size="16" /></span><div><strong>{{ stats.active }}<small>/{{ maxConcurrent }}</small></strong><p>Activas</p></div></div>
    <div class="summary-metric"><span class="metric-icon"><AppIcon name="clock" :size="16" /></span><div><strong>{{ stats.queued }}</strong><p>En cola</p></div></div>
    <div class="summary-metric"><span class="metric-icon"><AppIcon name="check" :size="16" /></span><div><strong>{{ stats.completed }}</strong><p>Completadas</p></div></div>
    <div v-if="stats.failed" class="summary-metric"><span class="metric-icon danger"><AppIcon name="warning" :size="16" /></span><div><strong>{{ stats.failed }}</strong><p>Con error</p></div></div>
  </section>
</template>

<style scoped>
.summary-strip { min-height: 76px; margin: 22px 30px 0; border: 1px solid var(--line); border-radius: 8px; background: linear-gradient(105deg,#111815,#0e1412); display: flex; align-items: stretch; overflow: hidden; }
.speed-block { min-width: 265px; padding: 0 24px; border-right: 1px solid var(--line); display: flex; align-items: center; gap: 16px; }.speed-block small { display: block; margin-bottom: 4px; color: #77817d; font: 9px var(--mono); letter-spacing: .12em; }.speed-block strong { font: 21px var(--mono); letter-spacing: -.04em; }
.pulse { height: 31px; width: 44px; display: flex; align-items: center; gap: 3px; }.pulse i { width: 3px; border-radius: 1px; background: var(--accent); animation: pulse 1.4s ease-in-out infinite; }.pulse i:nth-child(1),.pulse i:nth-child(5){height:8px;opacity:.35}.pulse i:nth-child(2),.pulse i:nth-child(4){height:19px;opacity:.65}.pulse i:nth-child(3){height:30px}.pulse i:nth-child(2){animation-delay:-.2s}.pulse i:nth-child(4){animation-delay:-.4s}
.summary-metric { min-width: 145px; padding: 0 24px; display: flex; align-items: center; gap: 11px; }.metric-icon { width: 32px; height: 32px; border-radius: 6px; background: #18201d; color: #7d8983; display: grid; place-items: center; }.metric-icon.active { background: rgba(194,255,91,.09); color: var(--accent); }.metric-icon.danger { color: var(--danger); }.summary-metric strong { display: block; font: 18px var(--mono); }.summary-metric strong small { color: #59625e; font-size: 11px; }.summary-metric p { margin: 2px 0 0; color: var(--muted); font-size: 10px; }
@keyframes pulse { 50% { transform: scaleY(.45); opacity:.4 } }
@media (max-width: 920px) { .summary-strip { overflow-x: auto; }.speed-block { min-width: 220px; }.summary-metric { min-width: 125px; padding: 0 16px; } }
@media (max-width: 760px) { .summary-strip { margin: 14px 16px 0; min-height: 67px; }.speed-block { min-width: 190px; padding: 0 16px; }.speed-block strong { font-size: 17px; }.summary-metric { min-width: 105px; }.metric-icon { display:none; } }
@container workspace (max-width:920px){.summary-strip{overflow-x:auto}.speed-block{min-width:220px}.summary-metric{min-width:125px;padding:0 16px}}
@container workspace (max-width:620px){.summary-strip{margin-left:16px;margin-right:16px;min-height:67px}.speed-block{min-width:190px;padding:0 16px}.speed-block strong{font-size:17px}.summary-metric{min-width:105px}.metric-icon{display:none}}
</style>
