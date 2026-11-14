<script setup lang="ts">
import { reactive, ref, watch } from "vue";
import type { ProxyProfile } from "@/domain/settings";
import { createId } from "@/domain/id";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{
  open: boolean;
  profiles: ProxyProfile[];
  nativeRuntime: boolean;
  busy: boolean;
}>();
const emit = defineEmits<{
  close: [];
  save: [profile: ProxyProfile];
  remove: [id: string];
  check: [id: string];
}>();
const editing = ref(false);
const form = reactive({ id: "", name: "", url: "", enabled: true });

watch(
  () => props.open,
  (open) => {
    if (open) reset();
  },
);

function reset(): void {
  editing.value = false;
  Object.assign(form, { id: "", name: "", url: "", enabled: true });
}
function edit(profile: ProxyProfile): void {
  editing.value = true;
  Object.assign(form, {
    id: profile.id,
    name: profile.name,
    url: profile.url,
    enabled: profile.enabled,
  });
}
function save(): void {
  emit("save", {
    id: form.id || createId(),
    name: form.name.trim(),
    url: form.url.trim(),
    enabled: form.enabled,
    health: { kind: "untested" },
  });
  reset();
}
function healthLabel(profile: ProxyProfile): string {
  if (profile.health.kind === "online") return `${profile.health.latencyMs} ms`;
  if (profile.health.kind === "offline") return "Sin conexión";
  if (profile.health.kind === "checking") return "Comprobando";
  return "Sin probar";
}
</script>

<template>
  <Teleport to="body"
    ><div v-if="open" class="dialog-backdrop ui-dialog-backdrop" @mousedown.self="emit('close')">
      <section
        class="dialog-card proxy-card ui-dialog-card max-w-2xl"
        role="dialog"
        aria-modal="true"
        aria-labelledby="proxy-title"
      >
        <header class="ui-dialog-header">
          <div>
            <p class="ui-eyebrow">PERFILES DE RED</p>
            <h2 class="ui-dialog-title" id="proxy-title">Proxies</h2>
          </div>
          <button class="ui-icon-button" type="button" aria-label="Cerrar" @click="emit('close')">
            <AppIcon name="close" />
          </button>
        </header>
        <div
          v-if="!nativeRuntime"
          class="web-note [margin:16px_20px_0] flex gap-[9px] rounded-lg border border-warning/40 bg-[rgba(220,163,70,.04)] p-2.5 text-warning"
        >
          <AppIcon name="warning" :size="16" />
          <p class="m-0 leading-[1.4] text-warning">
            <strong class="mb-0.5 block text-warning">Configuración en modo vista</strong>Los
            perfiles se guardan, pero solo el motor Rust puede utilizarlos.
          </p>
        </div>
        <div class="proxy-body min-h-0 overflow-y-auto p-5">
          <div
            v-if="profiles.length"
            class="proxy-list overflow-hidden rounded-lg border border-line"
          >
            <article
              class="grid min-h-15.5 grid-cols-[34px_minmax(120px,1fr)_95px_120px] items-center gap-2.5 border-b border-b-line px-2.5 py-2 last:border-0 max-dialog:grid-cols-[34px_1fr_auto]"
              v-for="profile in profiles"
              :key="profile.id"
            >
              <span
                class="proxy-icon grid h-[31px] w-[31px] place-items-center rounded-lg bg-surface text-muted"
                ><AppIcon name="proxy" :size="18"
              /></span>
              <div>
                <strong class="block text-caption">{{ profile.name }}</strong>
                <p
                  class="[margin:4px_0_0] max-w-65 overflow-hidden font-mono text-ellipsis whitespace-nowrap text-muted"
                >
                  {{ profile.url.replace(/:\/\/.*@/, "://••••@") }}
                </p>
              </div>
              <span
                class="health text-muted max-dialog:hidden [&.offline]:text-danger [&.online]:text-success"
                :class="profile.health.kind"
                ><i
                  class="mr-1.5 inline-block h-[5px] w-[5px] rounded-full bg-muted [.health.checking_&]:animate-[fluxor-blink_1s_infinite] [.health.checking_&]:bg-warning [.health.offline_&]:bg-danger [.health.online_&]:bg-success"
                ></i
                >{{ healthLabel(profile) }}</span
              >
              <div class="proxy-actions flex justify-end">
                <button
                  class="ui-icon-button size-9 border-transparent bg-transparent"
                  type="button"
                  title="Probar"
                  @click="emit('check', profile.id)"
                >
                  <AppIcon name="activity" :size="15" /></button
                ><button
                  class="ui-icon-button size-9 border-transparent bg-transparent"
                  type="button"
                  title="Editar"
                  @click="edit(profile)"
                >
                  <AppIcon name="settings" :size="15" /></button
                ><button
                  class="ui-icon-button size-9 border-transparent bg-transparent"
                  type="button"
                  title="Eliminar"
                  @click="emit('remove', profile.id)"
                >
                  <AppIcon name="trash" :size="15" />
                </button>
              </div>
            </article>
          </div>
          <div
            v-else
            class="no-proxies rounded-lg border border-dashed border-line p-[25px] text-center text-muted"
          >
            No hay perfiles configurados.
          </div>
          <form class="proxy-form mt-4.5 border-t border-t-line pt-[17px]" @submit.prevent="save">
            <h3 class="[margin:0_0_12px] text-caption">
              {{ editing ? "Editar perfil" : "Nuevo perfil" }}
            </h3>
            <div class="form-grid grid grid-cols-[160px_1fr] gap-2.5 max-dialog:grid-cols-[1fr]">
              <label
                ><span class="ui-label">NOMBRE</span
                ><input
                  class="ui-input w-full"
                  v-model="form.name"
                  name="proxy-name"
                  required
                  placeholder="Proxy oficina" /></label
              ><label
                ><span class="ui-label">URL</span
                ><input
                  class="ui-input w-full"
                  v-model="form.url"
                  name="proxy-url"
                  required
                  placeholder="socks5://usuario:clave@host:1080"
              /></label>
            </div>
            <label class="enable mt-[13px] flex cursor-pointer items-center gap-2 text-muted"
              ><input
                class="absolute opacity-0"
                v-model="form.enabled"
                name="proxy-enabled"
                type="checkbox"
              /><i
                class="h-[17px] w-7.5 rounded-xl bg-line p-0.5 after:block after:h-[13px] after:w-[13px] after:rounded-full after:bg-muted after:content-[''] after:[transition:.2s] [.enable_input:checked+&]:bg-accent [.enable_input:checked+&:after]:[transform:translateX(13px)] [.enable_input:checked+&:after]:bg-surface"
              ></i
              ><span>Perfil habilitado</span></label
            >
            <div class="form-actions mt-3.5 flex justify-end gap-[7px]">
              <button
                class="h-[33px] rounded-lg border border-line bg-raised px-[11px] py-0 text-muted disabled:opacity-45"
                v-if="editing"
                type="button"
                @click="reset"
              >
                Cancelar edición</button
              ><button
                class="save h-[33px] rounded-lg border-0 bg-accent px-[11px] py-0 font-bold text-surface disabled:opacity-45"
                type="submit"
                :disabled="busy || !form.name || !form.url"
              >
                {{ editing ? "Actualizar" : "Añadir perfil" }}
              </button>
            </div>
          </form>
        </div>
      </section>
    </div></Teleport
  >
</template>
