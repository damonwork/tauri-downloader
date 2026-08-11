<script setup lang="ts">
import AppIcon from "./AppIcon.vue";

defineProps<{ search: string; webMode: boolean }>();
const emit = defineEmits<{ add: []; settings: []; proxies: []; "update:search": [value: string] }>();
</script>

<template>
  <header class="top-bar">
    <div class="page-title">
      <p>ESPACIO DE TRABAJO</p>
      <h1>Descargas</h1>
    </div>
    <label class="search-box">
      <AppIcon name="search" :size="17" />
      <input :value="search" type="search" name="download-search" aria-label="Buscar descargas" placeholder="Buscar archivo u origen" @input="emit('update:search', ($event.target as HTMLInputElement).value)" />
      <kbd>⌘ K</kbd>
    </label>
    <div class="top-actions">
      <span v-if="webMode" class="runtime-chip"><i></i>Vista web</span>
      <button class="icon-button" type="button" aria-label="Perfiles proxy" @click="emit('proxies')"><AppIcon name="proxy" /></button>
      <button class="icon-button" type="button" aria-label="Preferencias" @click="emit('settings')"><AppIcon name="settings" /></button>
      <button class="add-button" type="button" aria-label="Nueva descarga" @click="emit('add')"><AppIcon name="plus" :size="18" /><span>Nueva descarga</span><kbd>⌘ N</kbd></button>
    </div>
  </header>
</template>

<style scoped>
.top-bar { min-height: 98px; padding: 23px 30px 19px; border-bottom: 1px solid var(--line); display: grid; grid-template-columns: minmax(170px,1fr) minmax(260px,440px) minmax(300px,1fr); align-items: center; gap: 24px; background: rgba(12,17,15,.86); backdrop-filter: blur(18px); position: sticky; top: 0; z-index: 20; }
.page-title p { margin: 0 0 3px; color: var(--muted); font: 9px var(--mono); letter-spacing: .15em; }.page-title h1 { margin: 0; font-size: 24px; line-height: 1; letter-spacing: -.02em; }
.search-box { height: 40px; border: 1px solid #252d29; border-radius: 7px; background: #0b100e; color: #69736e; display: flex; align-items: center; gap: 10px; padding: 0 11px; transition: border-color .2s; }.search-box:focus-within { border-color: #53634e; }.search-box input { min-width: 0; flex: 1; border: 0; outline: 0; background: transparent; color: var(--text); font: 12px var(--sans); }.search-box kbd,.add-button kbd { font: 9px var(--mono); color: #737d78; }
.top-actions { display: flex; justify-content: flex-end; gap: 8px; align-items: center; }.icon-button { width: 40px; height: 40px; border: 1px solid var(--line); border-radius: 7px; color: #9ca5a1; background: #101613; cursor: pointer; display: grid; place-items: center; }.icon-button:hover { color: var(--accent); border-color: #3b4938; }
.add-button { height: 40px; padding: 0 13px; border: 0; border-radius: 7px; background: var(--accent); color: #11170f; display: flex; align-items: center; gap: 8px; font-size: 11px; font-weight: 750; cursor: pointer; box-shadow: 0 0 0 1px rgba(255,255,255,.08) inset; }.add-button:hover { background: #d5ff83; }.add-button kbd { padding-left: 5px; color: #617d2e; }
.runtime-chip { border: 1px solid #29352f; padding: 7px 9px; border-radius: 5px; color: #84918b; font: 9px var(--mono); text-transform: uppercase; letter-spacing: .07em; }.runtime-chip i { display: inline-block; width: 5px; height: 5px; margin-right: 6px; border-radius: 50%; background: #e0a84b; }
@media (max-width: 1120px) { .top-bar { grid-template-columns: auto 1fr auto; }.runtime-chip,.add-button kbd { display: none; } }
@media (max-width: 760px) { .top-bar { min-height: auto; padding: 16px; display: flex; flex-wrap: wrap; gap: 8px; position: static; }.page-title { flex: 1; min-width: 120px; }.page-title h1 { font-size: 21px; }.search-box { order: 3; flex: 1 0 100%; margin-top: 4px; }.add-button { width: 40px; padding: 0; justify-content: center; }.add-button span { display: none; } }
@container workspace (max-width:900px){.top-bar{padding:20px;grid-template-columns:auto minmax(180px,1fr) auto;gap:12px}.runtime-chip,.add-button kbd,.top-actions>.icon-button{display:none}}
@container workspace (max-width:620px){.top-bar{min-height:auto;padding:16px;display:flex;flex-wrap:wrap;gap:8px}.page-title{flex:1;min-width:120px}.page-title h1{font-size:21px}.search-box{order:3;flex:1 0 100%;margin-top:4px}.top-actions{gap:6px}.add-button{height:38px}}
@media(max-width:760px){.top-actions>.icon-button{display:grid}.add-button{width:40px;padding:0;justify-content:center}.add-button span{display:none}}
</style>
