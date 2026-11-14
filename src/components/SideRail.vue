<script setup lang="ts">
import AppIcon from "./AppIcon.vue";

defineProps<{
  active: string;
  counts: { all: number; active: number; queued: number; completed: number };
}>();

const emit = defineEmits<{
  filter: [value: string];
  settings: [];
  proxies: [];
}>();

const navigation = [
  { id: "all", label: "Todas", icon: "grid" },
  { id: "active", label: "Activas", icon: "activity" },
  { id: "queued", label: "En espera", icon: "clock" },
  { id: "completed", label: "Completadas", icon: "check" },
];
</script>

<template>
  <aside
    class="side-rail flex min-h-screen w-58 shrink-0 grow-0 basis-58 flex-col border-r border-r-line bg-sidebar [padding:28px_18px_20px] max-mobile:fixed max-mobile:right-0 max-mobile:bottom-0 max-mobile:left-0 max-mobile:z-30 max-mobile:h-[65px] max-mobile:min-h-0 max-mobile:w-auto max-mobile:flex-row max-mobile:border-0 max-mobile:border-t max-mobile:border-t-line max-mobile:px-2 max-mobile:py-[7px]"
  >
    <div class="brand flex items-center gap-3 px-2 pb-8 max-mobile:hidden" aria-label="Fluxor">
      <div class="brand-mark flex h-9 w-7 shrink-0 -skew-x-9 items-end gap-1">
        <span
          class="block w-[7px] rounded-md bg-accent shadow-[0_0_14px_rgba(194,255,91,.22)] [&:nth-child(1)]:h-3.5 [&:nth-child(1)]:opacity-45 [&:nth-child(2)]:h-[23px] [&:nth-child(2)]:opacity-72 [&:nth-child(3)]:h-[31px]"
        ></span
        ><span
          class="block w-[7px] rounded-md bg-accent shadow-[0_0_14px_rgba(194,255,91,.22)] [&:nth-child(1)]:h-3.5 [&:nth-child(1)]:opacity-45 [&:nth-child(2)]:h-[23px] [&:nth-child(2)]:opacity-72 [&:nth-child(3)]:h-[31px]"
        ></span
        ><span
          class="block w-[7px] rounded-md bg-accent shadow-[0_0_14px_rgba(194,255,91,.22)] [&:nth-child(1)]:h-3.5 [&:nth-child(1)]:opacity-45 [&:nth-child(2)]:h-[23px] [&:nth-child(2)]:opacity-72 [&:nth-child(3)]:h-[31px]"
        ></span>
      </div>
      <div>
        <strong class="block text-sm tracking-[.16em]">FLUXOR</strong
        ><small class="mt-1 block font-sans text-xs text-muted">Gestor de descargas</small>
      </div>
    </div>

    <nav
      class="primary-nav grid gap-[5px] max-mobile:grid max-mobile:w-full max-mobile:grid-cols-[repeat(4,1fr)] max-mobile:gap-0.5"
      aria-label="Filtros de descarga"
    >
      <button
        v-for="item in navigation"
        :key="item.id"
        class="nav-item ui-nav-item"
        :class="{ active: active === item.id }"
        :aria-current="active === item.id ? 'page' : undefined"
        type="button"
        @click="emit('filter', item.id)"
      >
        <AppIcon :name="item.icon" :size="18" />
        <span>{{ item.label }}</span>
        <b
          class="min-w-[21px] rounded-md bg-line px-[5px] py-[3px] text-center font-mono text-foreground max-mobile:hidden"
          v-if="counts[item.id as keyof typeof counts]"
          >{{ counts[item.id as keyof typeof counts] }}</b
        >
      </button>
    </nav>

    <div class="nav-section mt-7 grid gap-[5px] max-mobile:hidden">
      <p class="[margin:0_10px_8px] font-mono tracking-[.14em] text-muted">CONEXIÓN</p>
      <button class="nav-item ui-nav-item" type="button" @click="emit('proxies')">
        <AppIcon name="proxy" :size="18" /><span>Perfiles proxy</span>
      </button>
      <button class="nav-item ui-nav-item" type="button" @click="emit('settings')">
        <AppIcon name="settings" :size="18" /><span>Preferencias</span>
      </button>
    </div>

    <div
      class="engine-status mt-auto flex items-center gap-2.5 rounded-lg border border-line bg-surface p-3 max-mobile:hidden"
    >
      <span
        class="status-light h-[7px] w-[7px] rounded-full bg-accent shadow-[0_0_10px_var(--color-accent)]"
      ></span>
      <div>
        <strong class="block text-caption font-semibold">Motor disponible</strong
        ><small class="mt-0.5 block text-muted">Cola sincronizada</small>
      </div>
    </div>
  </aside>
</template>
