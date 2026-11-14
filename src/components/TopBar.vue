<script setup lang="ts">
import AppIcon from "./AppIcon.vue";

defineProps<{ search: string; webMode: boolean }>();
const emit = defineEmits<{
  add: [];
  settings: [];
  proxies: [];
  "update:search": [value: string];
}>();
</script>

<template>
  <header
    class="top-bar sticky top-0 z-20 grid min-h-24.5 grid-cols-[minmax(170px,1fr)_minmax(260px,440px)_minmax(300px,1fr)] items-center gap-6 border-b border-b-line bg-[rgba(12,17,15,.86)] [padding:23px_30px_19px] backdrop-blur-[18px] max-[1120px]:grid-cols-[auto_1fr_auto] max-mobile:static max-mobile:flex max-mobile:min-h-auto max-mobile:flex-wrap max-mobile:gap-2 max-mobile:p-4 @max-[900px]/workspace:grid-cols-[auto_minmax(180px,1fr)_auto] @max-[900px]/workspace:gap-3 @max-[900px]/workspace:p-5 @max-[620px]/workspace:flex @max-[620px]/workspace:min-h-auto @max-[620px]/workspace:flex-wrap @max-[620px]/workspace:gap-2 @max-[620px]/workspace:p-4"
  >
    <div
      class="page-title max-mobile:min-w-30 max-mobile:flex-1 @max-[620px]/workspace:min-w-30 @max-[620px]/workspace:flex-1"
    >
      <p class="ui-eyebrow">ESPACIO DE TRABAJO</p>
      <h1
        class="m-0 text-2xl leading-[1] tracking-[-.02em] max-mobile:text-[21px] @max-[620px]/workspace:text-[21px]"
      >
        Descargas
      </h1>
    </div>
    <label
      class="search-box flex h-10 items-center gap-2.5 rounded-lg border border-line bg-canvas px-[11px] py-0 text-muted [transition:border-color_.2s] focus-within:border-muted max-mobile:[order:3] max-mobile:mt-1 max-mobile:[flex:1_0_100%] @max-[620px]/workspace:[order:3] @max-[620px]/workspace:mt-1 @max-[620px]/workspace:[flex:1_0_100%]"
    >
      <AppIcon name="search" :size="17" />
      <input
        class="min-w-0 flex-1 border-0 bg-transparent font-sans text-xs text-foreground outline-none"
        :value="search"
        type="search"
        name="download-search"
        aria-label="Buscar descargas"
        placeholder="Buscar archivo u origen"
        @input="emit('update:search', ($event.target as HTMLInputElement).value)"
      />
      <kbd class="font-mono text-muted">⌘ K</kbd>
    </label>
    <div class="top-actions flex items-center justify-end gap-2 @max-[620px]/workspace:gap-1.5">
      <span
        v-if="webMode"
        class="runtime-chip rounded-lg border border-line px-[9px] py-[7px] font-mono tracking-[.07em] text-muted uppercase max-[1120px]:hidden @max-[900px]/workspace:hidden"
        ><i class="mr-1.5 inline-block h-[5px] w-[5px] rounded-full bg-warning"></i>Vista web</span
      >
      <button
        class="icon-button ui-icon-button"
        type="button"
        aria-label="Perfiles proxy"
        @click="emit('proxies')"
      >
        <AppIcon name="proxy" />
      </button>
      <button
        class="icon-button ui-icon-button"
        type="button"
        aria-label="Preferencias"
        @click="emit('settings')"
      >
        <AppIcon name="settings" />
      </button>
      <button
        class="add-button ui-button ui-button-primary max-mobile:size-10 max-mobile:p-0"
        type="button"
        aria-label="Nueva descarga"
        @click="emit('add')"
      >
        <AppIcon name="plus" :size="18" /><span class="max-mobile:hidden">Nueva descarga</span
        ><kbd
          class="pl-[5px] font-mono text-success max-[1120px]:hidden @max-[900px]/workspace:hidden"
          >⌘ N</kbd
        >
      </button>
    </div>
  </header>
</template>
