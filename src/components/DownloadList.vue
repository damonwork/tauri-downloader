<script setup lang="ts">
import type { DownloadAction, DownloadItem } from "@/domain/download";
import DownloadRow from "./DownloadRow.vue";
import AppIcon from "./AppIcon.vue";

defineProps<{
  items: DownloadItem[];
  selectedId?: string;
  filterLabel: string;
  canReveal: boolean;
  hasItems: boolean;
  searchActive: boolean;
}>();
const emit = defineEmits<{
  select: [id: string];
  action: [id: string, action: DownloadAction];
  reveal: [id: string];
  add: [];
}>();
</script>

<template>
  <section
    class="list-panel [margin:18px_30px_30px] overflow-hidden rounded-lg border border-line bg-surface max-mobile:[margin:14px_16px_82px] @max-[650px]/workspace:mr-4 @max-[650px]/workspace:ml-4"
  >
    <header
      class="list-header flex h-13.5 items-center justify-between border-b border-b-line px-[15px] py-0 max-mobile:h-12 @max-[650px]/workspace:h-12 max-mobile:[&_button]:hidden"
    >
      <div>
        <strong class="text-xs">{{ filterLabel }}</strong><span
          class="ml-[9px] font-mono text-muted"
          >{{ items.length }} elementos</span
        >
      </div>
    </header>
    <div
      v-if="items.length"
      class="column-labels grid min-h-[31px] download-columns items-center gap-4.5 border-b border-b-line px-[15px] py-0 font-mono tracking-[.12em] text-muted max-mobile:hidden @max-[1100px]/workspace:gap-3 @max-[650px]/workspace:hidden"
      aria-hidden="true"
    >
      <span
        class="max-[1100px]:[&:nth-child(2)]:hidden @max-[1100px]/workspace:[&:nth-child(2)]:hidden"
        >ARCHIVO</span
      ><span
        class="max-[1100px]:[&:nth-child(2)]:hidden @max-[1100px]/workspace:[&:nth-child(2)]:hidden"
        >ESTADO</span
      ><span
        class="max-[1100px]:[&:nth-child(2)]:hidden @max-[1100px]/workspace:[&:nth-child(2)]:hidden"
        >PROGRESO</span
      ><span
        class="max-[1100px]:[&:nth-child(2)]:hidden @max-[1100px]/workspace:[&:nth-child(2)]:hidden"
        >TRANSFERENCIA</span
      ><span
        class="max-[1100px]:[&:nth-child(2)]:hidden @max-[1100px]/workspace:[&:nth-child(2)]:hidden"
      ></span>
    </div>
    <div v-if="items.length" class="rows">
      <DownloadRow
        v-for="item in items"
        :key="item.id"
        :item="item"
        :selected="item.id === selectedId"
        :can-reveal="canReveal"
        @select="emit('select', item.id)"
        @action="emit('action', item.id, $event)"
        @reveal="emit('reveal', item.id)"
      />
    </div>
    <div v-else class="empty-state flex min-h-90 flex-col items-center justify-center text-center">
      <span class="grid h-14.5 w-14.5 place-items-center rounded-xl bg-surface text-muted"
        ><AppIcon name="download" :size="27"
      /></span>
      <h2 class="[margin:18px_0_5px] text-base">
        {{ hasItems ? "Sin resultados" : "No hay descargas aquí" }}
      </h2>
      <p class="m-0 text-muted">
        {{
          hasItems
            ? searchActive
              ? "Prueba con otro nombre o dominio."
              : "No hay elementos en este filtro."
            : "Pega un enlace o importa un comando cURL para comenzar."
        }}
      </p>
      <button
        class="mt-5 flex h-[37px] cursor-pointer items-center gap-[7px] rounded-lg border-0 bg-accent px-[13px] py-0 font-bold text-surface"
        v-if="!hasItems"
        type="button"
        @click="emit('add')"
      >
        <AppIcon name="plus" :size="17" />Añadir descarga
      </button>
    </div>
  </section>
</template>
