<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import type { AppSettings, CategoryDirectories } from "@/domain/settings";
import AppIcon from "./AppIcon.vue";
import SpeedLimitInput from "./SpeedLimitInput.vue";

const props = defineProps<{ open: boolean; settings: AppSettings; busy: boolean }>();
const emit = defineEmits<{ close: []; save: [settings: AppSettings] }>();
const form = reactive<AppSettings>(cloneSettings(props.settings));
const activeSection = ref<"general" | "folders">("general");
const categoryFields: { key: keyof CategoryDirectories; label: string; description: string }[] = [
  { key: "video", label: "Videos", description: "MP4, MKV, MOV, WEBM" },
  { key: "archive", label: "Comprimidos", description: "ZIP, RAR, 7Z, TAR" },
  { key: "document", label: "Documentos", description: "PDF, DOCX, XLSX, TXT" },
  { key: "audio", label: "Audio", description: "MP3, FLAC, WAV, M4A" },
  { key: "other", label: "Otros", description: "Archivos sin categoría" },
];

const rootIsAbsolute = computed(() => /^[a-zA-Z]:[\\/]|^\//.test(form.downloadDirectory.trim()));
const rootPreview = computed(() => rootIsAbsolute.value
  ? form.downloadDirectory
  : `Descargas / ${form.downloadDirectory}`,
);

watch(() => props.open, (open) => {
  if (!open) return;
  Object.assign(form, cloneSettings(props.settings));
  activeSection.value = "general";
});

function submit(): void {
  emit("save", cloneSettings(form));
}

function cloneSettings(settings: AppSettings): AppSettings {
  return {
    maxConcurrent: settings.maxConcurrent,
    defaultThreads: settings.defaultThreads,
    defaultSpeedLimitBytes: settings.defaultSpeedLimitBytes,
    downloadDirectory: settings.downloadDirectory,
    organizeByCategory: settings.organizeByCategory,
    categoryDirectories: { ...settings.categoryDirectories },
    startImmediately: settings.startImmediately,
  };
}
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="dialog-backdrop" @mousedown.self="emit('close')">
      <section class="settings-card" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <header>
          <div><p>PREFERENCIAS</p><h2 id="settings-title">Configurar Fluxor</h2></div>
          <button type="button" aria-label="Cerrar" @click="emit('close')"><AppIcon name="close" /></button>
        </header>

        <div class="settings-layout">
          <nav aria-label="Secciones de preferencias">
            <button type="button" :class="{ active: activeSection === 'general' }" @click="activeSection='general'"><AppIcon name="settings" :size="17" /><span><strong>General</strong><small>Motor y comportamiento</small></span></button>
            <button type="button" :class="{ active: activeSection === 'folders' }" @click="activeSection='folders'"><AppIcon name="folder" :size="17" /><span><strong>Carpetas</strong><small>Destino por categoría</small></span></button>
          </nav>

          <form @submit.prevent="submit">
            <div class="settings-body">
              <section v-if="activeSection === 'general'">
                <div class="section-heading"><span><AppIcon name="bolt" :size="18" /></span><div><h3>Motor de transferencia</h3><p>Valores predeterminados para nuevas descargas.</p></div></div>
                <div class="setting-row"><div><strong>Descargas simultáneas</strong><p>Archivos que pueden transferirse al mismo tiempo.</p></div><div class="stepper"><button type="button" @click="form.maxConcurrent=Math.max(1,form.maxConcurrent-1)">−</button><input v-model.number="form.maxConcurrent" name="max-concurrent" type="number" min="1" max="12" /><button type="button" @click="form.maxConcurrent=Math.min(12,form.maxConcurrent+1)">+</button></div></div>
                <div class="setting-row"><div><strong>Máximo de segmentos</strong><p>Es un límite: Fluxor usa menos si el servidor no los tolera.</p></div><select v-model.number="form.defaultThreads" name="default-threads"><option v-for="value in [1,2,4,6,8,12,16,24,32]" :key="value" :value="value">Hasta {{ value }}</option></select></div>
                <div class="setting-row"><div><strong>Límite de velocidad</strong><p>Tope agregado predeterminado para cada descarga.</p></div><SpeedLimitInput v-model="form.defaultSpeedLimitBytes" class="setting-speed" name="default-speed-limit" /></div>
                <label class="toggle-row"><div><strong>Iniciar al añadir</strong><p>Las nuevas transferencias entran directamente en la cola.</p></div><input v-model="form.startImmediately" name="start-immediately" type="checkbox" /><i></i></label>
                <div class="info-card"><AppIcon name="activity" :size="18" /><p><strong>Configuración individual</strong>Cada descarga puede sobrescribir categoría, destino, segmentos, proxy, cookies y headers desde el diálogo de alta.</p></div>
              </section>

              <section v-else>
                <div class="section-heading"><span><AppIcon name="folder" :size="18" /></span><div><h3>Organización de archivos</h3><p>La raíz relativa se crea dentro de Descargas del sistema.</p></div></div>
                <label class="root-field"><span>CARPETA PRINCIPAL</span><div><AppIcon name="folder" :size="16" /><input v-model="form.downloadDirectory" name="download-directory" placeholder="Fluxor o C:\\Mi ruta" /></div><small>Resultado: {{ rootPreview }}</small></label>
                <label class="toggle-row organize-toggle"><div><strong>Organizar por categoría</strong><p>Crear automáticamente una subcarpeta según el tipo.</p></div><input v-model="form.organizeByCategory" name="organize-categories" type="checkbox" /><i></i></label>
                <div class="category-folders" :class="{ disabled: !form.organizeByCategory }">
                  <label v-for="field in categoryFields" :key="field.key"><span class="category-icon"><AppIcon name="folder" :size="15" /></span><span class="category-copy"><strong>{{ field.label }}</strong><small>{{ field.description }}</small></span><span class="path-prefix">/</span><input v-model="form.categoryDirectories[field.key]" :name="`folder-${field.key}`" :disabled="!form.organizeByCategory" /></label>
                </div>
              </section>
            </div>

            <footer><p v-if="activeSection === 'folders'"><AppIcon name="shield" :size="14" />Las rutas se validan antes de crear archivos.</p><div><button type="button" @click="emit('close')">Cancelar</button><button class="save" type="submit" :disabled="busy">{{ busy ? 'Guardando…' : 'Guardar cambios' }}</button></div></footer>
          </form>
        </div>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.dialog-backdrop{position:fixed;inset:0;z-index:100;background:rgba(2,5,4,.78);backdrop-filter:blur(9px);display:grid;place-items:center;padding:20px}.settings-card{width:min(720px,100%);max-height:calc(100vh - 40px);overflow:hidden;border:1px solid #303a35;border-radius:11px;background:#0e1411;box-shadow:0 30px 100px rgba(0,0,0,.58)}header{height:72px;padding:0 22px;border-bottom:1px solid var(--line);display:flex;align-items:center;justify-content:space-between}header p{margin:0 0 4px;color:#65706a;font:8px var(--mono);letter-spacing:.14em}h2{margin:0;font-size:17px}header>button{width:32px;height:32px;border:0;background:transparent;color:#77817c;cursor:pointer}.settings-layout{display:grid;grid-template-columns:174px 1fr;min-height:500px}.settings-layout>nav{padding:16px 10px;border-right:1px solid var(--line);background:#0b100e}.settings-layout>nav button{width:100%;min-height:54px;padding:9px 10px;border:0;border-radius:7px;background:transparent;color:#6f7974;display:flex;align-items:center;gap:10px;text-align:left;cursor:pointer}.settings-layout>nav button.active{background:#17201b;color:var(--accent);box-shadow:inset 2px 0 var(--accent)}.settings-layout>nav strong,.settings-layout>nav small{display:block}.settings-layout>nav strong{color:inherit;font-size:10px}.settings-layout>nav small{margin-top:3px;color:#5e6863;font-size:8px}.settings-layout>form{min-width:0;display:flex;flex-direction:column}.settings-body{height:437px;padding:21px 24px;overflow:auto}.section-heading{display:flex;align-items:center;gap:11px;margin-bottom:13px}.section-heading>span{width:36px;height:36px;border-radius:7px;background:rgba(194,255,91,.08);color:var(--accent);display:grid;place-items:center}.section-heading h3{margin:0;font-size:13px}.section-heading p{margin:4px 0 0;color:#626c67;font-size:8px}.setting-row,.toggle-row{min-height:72px;border-bottom:1px solid #202824;display:flex;align-items:center;justify-content:space-between;gap:20px}.setting-row strong,.toggle-row strong{display:block;font-size:10px}.setting-row p,.toggle-row p{margin:4px 0 0;color:#606a65;font-size:8px}.setting-row select{width:105px;height:35px;padding:0 8px;border:1px solid #2b3530;border-radius:5px;background:#090e0c;color:#c8d0cc;font-size:9px}.stepper{height:34px;border:1px solid #2b3530;border-radius:5px;display:flex;overflow:hidden}.stepper button{width:29px;border:0;background:#131a17;color:#75817b;cursor:pointer}.stepper input{width:38px;border:0;border-left:1px solid #242e29;border-right:1px solid #242e29;background:#090e0c;color:#d1d8d4;text-align:center;font:10px var(--mono);appearance:textfield}.toggle-row{position:relative;cursor:pointer}.toggle-row input{position:absolute;opacity:0}.toggle-row>i{width:34px;height:19px;padding:3px;border-radius:12px;background:#29322e;flex:0 0 auto}.toggle-row>i::after{content:"";display:block;width:13px;height:13px;border-radius:50%;background:#7a847f;transition:.2s}.toggle-row input:checked+i{background:var(--accent)}.toggle-row input:checked+i::after{transform:translateX(15px);background:#11170f}.info-card{margin-top:20px;padding:13px;border:1px solid #29352e;border-radius:7px;background:rgba(194,255,91,.025);color:var(--accent);display:flex;gap:10px}.info-card p{margin:0;color:#69736e;font-size:8px;line-height:1.5}.info-card strong{display:block;margin-bottom:2px;color:#aab4ae;font-size:9px}.root-field{display:block;margin-top:18px}.root-field>span{display:block;margin-bottom:7px;color:#6d7772;font:8px var(--mono);letter-spacing:.1em}.root-field>div{position:relative}.root-field .app-icon{position:absolute;left:10px;top:10px;color:#69736e}.root-field input{width:100%;height:37px;padding:0 10px 0 34px;border:1px solid #2b3530;border-radius:5px;background:#090e0c;color:#c8d0cc;font:9px var(--mono)}.root-field small{display:block;margin-top:6px;color:#68726d;font:8px var(--mono)}.organize-toggle{min-height:62px;margin-top:8px}.category-folders{margin-top:10px;border:1px solid #222c27;border-radius:7px;overflow:hidden;transition:opacity .2s}.category-folders.disabled{opacity:.42}.category-folders label{min-height:53px;padding:7px 9px;border-bottom:1px solid #202824;display:grid;grid-template-columns:29px 1fr 10px 135px;align-items:center;gap:8px}.category-folders label:last-child{border:0}.category-icon{width:27px;height:27px;border-radius:5px;background:#19211d;color:#79857f;display:grid;place-items:center}.category-copy strong,.category-copy small{display:block}.category-copy strong{font-size:9px}.category-copy small{margin-top:2px;color:#58625d;font-size:7px}.path-prefix{color:#4f5954;font:10px var(--mono)}.category-folders input{width:100%;height:31px;padding:0 8px;border:1px solid #29332e;border-radius:4px;background:#090e0c;color:#b9c2bd;font:9px var(--mono)}footer{height:63px;padding:0 22px;border-top:1px solid var(--line);display:flex;align-items:center;justify-content:space-between;gap:12px}footer p{margin:0;color:#65706a;font-size:8px;display:flex;align-items:center;gap:6px}footer>div{margin-left:auto;display:flex;gap:7px}footer button{height:34px;padding:0 12px;border:1px solid #29332e;border-radius:6px;background:#121815;color:#87918c;font-size:9px;cursor:pointer}footer button.save{border:0;background:var(--accent);color:#11170f;font-weight:700}
.setting-speed{width:190px;flex:0 0 190px}
@media(max-width:620px){.dialog-backdrop{padding:0;align-items:end}.settings-card{max-height:95vh;border-radius:12px 12px 0 0}.settings-layout{display:block}.settings-layout>nav{padding:8px 14px;border:0;border-bottom:1px solid var(--line);display:grid;grid-template-columns:1fr 1fr;gap:6px}.settings-layout>nav button{min-height:45px}.settings-body{height:calc(95vh - 190px);padding:18px}.category-folders label{grid-template-columns:29px 1fr 8px 105px}footer p{display:none}.setting-speed{width:170px;flex-basis:170px}}
</style>
