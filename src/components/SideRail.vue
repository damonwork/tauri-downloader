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
  <aside class="side-rail">
    <div class="brand" aria-label="Fluxor">
      <div class="brand-mark"><span></span><span></span><span></span></div>
      <div><strong>FLUXOR</strong><small>TRANSFER ENGINE</small></div>
    </div>

    <nav class="primary-nav" aria-label="Filtros de descarga">
      <button
        v-for="item in navigation"
        :key="item.id"
        class="nav-item"
        :class="{ active: active === item.id }"
        type="button"
        @click="emit('filter', item.id)"
      >
        <AppIcon :name="item.icon" :size="18" />
        <span>{{ item.label }}</span>
        <b v-if="counts[item.id as keyof typeof counts]">{{ counts[item.id as keyof typeof counts] }}</b>
      </button>
    </nav>

    <div class="nav-section">
      <p>CONEXIÓN</p>
      <button class="nav-item" type="button" @click="emit('proxies')">
        <AppIcon name="proxy" :size="18" /><span>Perfiles proxy</span>
      </button>
      <button class="nav-item" type="button" @click="emit('settings')">
        <AppIcon name="settings" :size="18" /><span>Preferencias</span>
      </button>
    </div>

    <div class="engine-status">
      <span class="status-light"></span>
      <div><strong>Motor disponible</strong><small>Cola sincronizada</small></div>
    </div>
  </aside>
</template>

<style scoped>
.side-rail { width: 232px; flex: 0 0 232px; min-height: 100vh; padding: 28px 18px 20px; border-right: 1px solid var(--line); background: #090d0c; display: flex; flex-direction: column; }
.brand { display: flex; align-items: center; gap: 11px; padding: 0 10px 32px; }
.brand-mark { width: 31px; height: 31px; display: flex; align-items: flex-end; gap: 3px; transform: skew(-9deg); }
.brand-mark span { display: block; width: 7px; background: var(--accent); border-radius: 1px; box-shadow: 0 0 14px rgba(194,255,91,.22); }
.brand-mark span:nth-child(1) { height: 14px; opacity: .45; }.brand-mark span:nth-child(2) { height: 23px; opacity: .72; }.brand-mark span:nth-child(3) { height: 31px; }
.brand strong { display: block; letter-spacing: .16em; font-size: 14px; }.brand small { display: block; margin-top: 2px; color: var(--muted); font: 8px/1.2 var(--mono); letter-spacing: .16em; }
.primary-nav, .nav-section { display: grid; gap: 5px; }.nav-section { margin-top: 28px; }.nav-section > p { margin: 0 10px 8px; color: #7f8a84; font: 10px var(--mono); letter-spacing: .14em; }
.nav-item { width: 100%; min-height: 42px; border: 0; border-radius: 7px; background: transparent; color: #89938f; display: grid; grid-template-columns: 24px 1fr auto; align-items: center; gap: 8px; padding: 0 11px; font-size: 13px; text-align: left; cursor: pointer; transition: .18s ease; }
.nav-item:hover { color: var(--text); background: rgba(255,255,255,.035); }.nav-item.active { color: var(--text); background: #171e1b; box-shadow: inset 2px 0 var(--accent); }.nav-item.active :deep(.app-icon) { color: var(--accent); }
.nav-item b { min-width: 21px; padding: 3px 5px; border-radius: 4px; background: #202824; color: #aeb7b3; font: 10px var(--mono); text-align: center; }
.engine-status { margin-top: auto; border: 1px solid var(--line); background: #0e1311; border-radius: 8px; padding: 12px; display: flex; align-items: center; gap: 10px; }.status-light { width: 7px; height: 7px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 10px var(--accent); }.engine-status strong,.engine-status small { display: block; }.engine-status strong { font-size: 11px; font-weight: 600; }.engine-status small { color: var(--muted); font-size: 9px; margin-top: 2px; }
@media (max-width: 760px) { .side-rail { position: fixed; z-index: 30; bottom: 0; left: 0; right: 0; width: auto; min-height: 0; height: 65px; padding: 7px 8px; border: 0; border-top: 1px solid var(--line); flex-direction: row; }.brand,.nav-section,.engine-status { display: none; }.primary-nav { width: 100%; display: grid; grid-template-columns: repeat(4,1fr); gap: 2px; }.nav-item { display: flex; min-height: 50px; padding: 4px; gap: 3px; flex-direction: column; justify-content: center; font-size: 10px; text-align: center; }.nav-item b { display: none; }.nav-item.active { box-shadow: inset 0 2px var(--accent); } }
</style>
