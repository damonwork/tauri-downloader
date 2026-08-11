<script setup lang="ts">
import type { DownloadAction, DownloadItem } from "@/domain/download";
import DownloadRow from "./DownloadRow.vue";
import AppIcon from "./AppIcon.vue";

defineProps<{ items: DownloadItem[]; selectedId?: string; filterLabel: string }>();
const emit = defineEmits<{ select: [id: string]; action: [id: string, action: DownloadAction]; add: [] }>();
</script>

<template>
  <section class="list-panel">
    <header class="list-header">
      <div><strong>{{ filterLabel }}</strong><span>{{ items.length }} elementos</span></div>
    </header>
    <div v-if="items.length" class="column-labels" aria-hidden="true"><span>ARCHIVO</span><span>ESTADO</span><span>PROGRESO</span><span>TRANSFERENCIA</span><span></span></div>
    <div v-if="items.length" class="rows">
      <DownloadRow
        v-for="item in items"
        :key="item.id"
        :item="item"
        :selected="item.id === selectedId"
        @select="emit('select', item.id)"
        @action="emit('action', item.id, $event)"
      />
    </div>
    <div v-else class="empty-state">
      <span><AppIcon name="download" :size="27" /></span>
      <h2>No hay descargas aquí</h2>
      <p>Pega un enlace o importa un comando cURL para comenzar.</p>
      <button type="button" @click="emit('add')"><AppIcon name="plus" :size="17" />Añadir descarga</button>
    </div>
  </section>
</template>

<style scoped>
.list-panel { margin:18px 30px 30px;border:1px solid var(--line);border-radius:8px;background:#0d1210;overflow:hidden }.list-header { height:54px;padding:0 15px;border-bottom:1px solid var(--line);display:flex;align-items:center;justify-content:space-between }.list-header strong { font-size:12px }.list-header span { margin-left:9px;color:#7b8580;font:9px var(--mono) }.column-labels { min-height:31px;padding:0 15px;display:grid;grid-template-columns:minmax(255px,1.35fr) 115px minmax(210px,1fr) 125px 104px;gap:18px;align-items:center;border-bottom:1px solid var(--line);color:#515b56;font:8px var(--mono);letter-spacing:.12em }.empty-state { min-height:360px;display:flex;align-items:center;justify-content:center;flex-direction:column;text-align:center }.empty-state>span{width:58px;height:58px;border-radius:10px;background:#161d1a;color:#65716b;display:grid;place-items:center}.empty-state h2{margin:18px 0 5px;font-size:16px}.empty-state p{margin:0;color:var(--muted);font-size:11px}.empty-state button{margin-top:20px;height:37px;border:0;border-radius:6px;background:var(--accent);color:#10160e;padding:0 13px;display:flex;align-items:center;gap:7px;font-size:10px;font-weight:700;cursor:pointer}
@media(max-width:1100px){.column-labels{grid-template-columns:minmax(250px,1.3fr) minmax(180px,1fr) 105px 68px}.column-labels span:nth-child(2){display:none}}
@media(max-width:760px){.list-panel{margin:14px 16px 82px}.column-labels{display:none}.list-header{height:48px}.list-header button{display:none}}
@container workspace (max-width:1100px){.column-labels{grid-template-columns:minmax(180px,1.3fr) minmax(130px,1fr) 90px 68px;gap:12px}.column-labels span:nth-child(2){display:none}}
@container workspace (max-width:650px){.list-panel{margin-left:16px;margin-right:16px}.column-labels{display:none}.list-header{height:48px}}
</style>
