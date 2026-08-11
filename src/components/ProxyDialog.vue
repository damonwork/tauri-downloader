<script setup lang="ts">
import { reactive, ref, watch } from "vue";
import type { ProxyProfile } from "@/domain/settings";
import { createId } from "@/domain/id";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{ open: boolean; profiles: ProxyProfile[]; nativeRuntime: boolean; busy: boolean }>();
const emit = defineEmits<{ close: []; save: [profile: ProxyProfile]; remove: [id: string]; check: [id: string] }>();
const editing = ref(false);
const form = reactive({ id: "", name: "", url: "", enabled: true });

watch(() => props.open, (open) => { if (open) reset(); });

function reset(): void { editing.value = false; Object.assign(form, { id: "", name: "", url: "", enabled: true }); }
function edit(profile: ProxyProfile): void { editing.value = true; Object.assign(form, { id: profile.id, name: profile.name, url: profile.url, enabled: profile.enabled }); }
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
  <Teleport to="body"><div v-if="open" class="dialog-backdrop" @mousedown.self="emit('close')"><section class="dialog-card proxy-card" role="dialog" aria-modal="true" aria-labelledby="proxy-title"><header><div><p>PERFILES DE RED</p><h2 id="proxy-title">Proxies</h2></div><button type="button" aria-label="Cerrar" @click="emit('close')"><AppIcon name="close" /></button></header><div v-if="!nativeRuntime" class="web-note"><AppIcon name="warning" :size="16" /><p><strong>Configuración en modo vista</strong>Los perfiles se guardan, pero solo el motor Rust puede utilizarlos.</p></div><div class="proxy-body"><div v-if="profiles.length" class="proxy-list"><article v-for="profile in profiles" :key="profile.id"><span class="proxy-icon"><AppIcon name="proxy" :size="18" /></span><div><strong>{{ profile.name }}</strong><p>{{ profile.url.replace(/:\/\/.*@/, '://••••@') }}</p></div><span class="health" :class="profile.health.kind"><i></i>{{ healthLabel(profile) }}</span><div class="proxy-actions"><button type="button" title="Probar" @click="emit('check',profile.id)"><AppIcon name="activity" :size="15" /></button><button type="button" title="Editar" @click="edit(profile)"><AppIcon name="settings" :size="15" /></button><button type="button" title="Eliminar" @click="emit('remove',profile.id)"><AppIcon name="trash" :size="15" /></button></div></article></div><div v-else class="no-proxies">No hay perfiles configurados.</div><form class="proxy-form" @submit.prevent="save"><h3>{{ editing ? 'Editar perfil' : 'Nuevo perfil' }}</h3><div class="form-grid"><label><span>NOMBRE</span><input v-model="form.name" name="proxy-name" required placeholder="Proxy oficina" /></label><label><span>URL</span><input v-model="form.url" name="proxy-url" required placeholder="socks5://usuario:clave@host:1080" /></label></div><label class="enable"><input v-model="form.enabled" name="proxy-enabled" type="checkbox" /><i></i><span>Perfil habilitado</span></label><div class="form-actions"><button v-if="editing" type="button" @click="reset">Cancelar edición</button><button class="save" type="submit" :disabled="busy || !form.name || !form.url">{{ editing ? 'Actualizar' : 'Añadir perfil' }}</button></div></form></div></section></div></Teleport>
</template>

<style scoped>
.dialog-backdrop{position:fixed;inset:0;z-index:100;background:rgba(2,5,4,.76);backdrop-filter:blur(8px);display:grid;place-items:center;padding:20px}.dialog-card{width:min(660px,100%);max-height:calc(100vh - 40px);overflow:auto;border:1px solid #303a35;border-radius:10px;background:#0e1411;box-shadow:0 30px 100px rgba(0,0,0,.55)}header{height:72px;padding:0 22px;border-bottom:1px solid var(--line);display:flex;align-items:center;justify-content:space-between}header p{margin:0 0 4px;color:#65706a;font:8px var(--mono);letter-spacing:.14em}h2{margin:0;font-size:17px}header button{width:32px;height:32px;border:0;background:transparent;color:#77817c}.web-note{margin:16px 20px 0;padding:10px;border:1px solid #4b3d26;border-radius:6px;background:rgba(220,163,70,.04);color:#d6a755;display:flex;gap:9px}.web-note strong{display:block;margin-bottom:2px;color:#ac8a53;font-size:9px}.web-note p{margin:0;color:#7d6b4d;font-size:8px;line-height:1.4}.proxy-body{padding:16px 20px 22px}.proxy-list{border:1px solid #202925;border-radius:7px;overflow:hidden}.proxy-list article{min-height:62px;padding:8px 10px;border-bottom:1px solid #202925;display:grid;grid-template-columns:34px minmax(150px,1fr) 95px 86px;gap:10px;align-items:center}.proxy-list article:last-child{border:0}.proxy-icon{width:31px;height:31px;border-radius:5px;background:#19211d;color:#8a9690;display:grid;place-items:center}.proxy-list strong{display:block;font-size:9px}.proxy-list p{margin:4px 0 0;max-width:260px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:#606a65;font:8px var(--mono)}.health{color:#6c7671;font-size:8px}.health i{display:inline-block;width:5px;height:5px;margin-right:6px;border-radius:50%;background:#69736e}.health.online{color:#78c9a8}.health.online i{background:#65cda5}.health.offline{color:#b47972}.health.offline i{background:var(--danger)}.health.checking i{background:#d4a34e;animation:blink 1s infinite}.proxy-actions{display:flex;justify-content:flex-end}.proxy-actions button{width:27px;height:27px;border:0;background:transparent;color:#66716b;display:grid;place-items:center}.proxy-actions button:hover{color:var(--accent)}.no-proxies{padding:25px;border:1px dashed #29322e;border-radius:7px;text-align:center;color:#626c67;font-size:9px}.proxy-form{margin-top:18px;padding-top:17px;border-top:1px solid #202925}.proxy-form h3{margin:0 0 12px;font-size:11px}.form-grid{display:grid;grid-template-columns:160px 1fr;gap:10px}.form-grid label span{display:block;margin-bottom:6px;color:#6b7670;font:8px var(--mono);letter-spacing:.08em}.form-grid input{width:100%;height:36px;padding:0 9px;border:1px solid #2a342f;border-radius:5px;background:#090e0c;color:#c4ccc8;font:9px var(--mono);outline:0}.enable{margin-top:13px;display:flex;align-items:center;gap:8px;color:#87918c;font-size:9px;cursor:pointer}.enable input{position:absolute;opacity:0}.enable i{width:30px;height:17px;padding:2px;border-radius:10px;background:#29322e}.enable i:after{content:"";display:block;width:13px;height:13px;border-radius:50%;background:#78827d;transition:.2s}.enable input:checked+i{background:var(--accent)}.enable input:checked+i:after{background:#11170f;transform:translateX(13px)}.form-actions{margin-top:14px;display:flex;justify-content:flex-end;gap:7px}.form-actions button{height:33px;padding:0 11px;border:1px solid #2b3530;border-radius:5px;background:#121815;color:#7f8984;font-size:9px}.form-actions button.save{border:0;background:var(--accent);color:#11170f;font-weight:700}.form-actions button:disabled{opacity:.45}@keyframes blink{50%{opacity:.25}}
@media(max-width:600px){.dialog-backdrop{padding:0;align-items:end}.dialog-card{max-height:94vh;border-radius:12px 12px 0 0}.proxy-list article{grid-template-columns:34px 1fr auto}.health{display:none}.proxy-actions button:first-child{display:none}.form-grid{grid-template-columns:1fr}}
</style>
