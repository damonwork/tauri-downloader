<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from "vue";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{
  open: boolean;
  eyebrow: string;
  title: string;
  message: string;
  confirmLabel: string;
  tone: "warning" | "danger";
  busy: boolean;
}>();
const emit = defineEmits<{ cancel: []; confirm: [] }>();
const dialog = ref<HTMLElement>();
const backdrop = ref<HTMLElement>();
const cancelButton = ref<HTMLButtonElement>();
const previousFocus = ref<HTMLElement>();
const inertedElements = new Map<HTMLElement, boolean>();

watch(() => props.open, async (open) => {
  if (open) {
    previousFocus.value = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    await nextTick();
    for (const element of Array.from(document.body.children)) {
      if (!(element instanceof HTMLElement) || element === backdrop.value) continue;
      inertedElements.set(element, element.inert);
      element.inert = true;
    }
    cancelButton.value?.focus();
    return;
  }
  restoreBackground();
  await nextTick();
  if (previousFocus.value?.isConnected) previousFocus.value.focus();
  else document.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
  previousFocus.value = undefined;
});

watch(() => props.busy, async (busy) => {
  if (!busy || !props.open) return;
  await nextTick();
  dialog.value?.focus();
});

onBeforeUnmount(restoreBackground);

function cancel(): void {
  if (!props.busy) emit("cancel");
}

function handleKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    event.preventDefault();
    cancel();
    return;
  }
  if (event.key !== "Tab") return;
  const controls = Array.from(dialog.value?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []);
  if (controls.length === 0) {
    event.preventDefault();
    dialog.value?.focus();
    return;
  }
  const current = controls.indexOf(document.activeElement as HTMLButtonElement);
  const next = event.shiftKey
    ? (current <= 0 ? controls.length - 1 : current - 1)
    : (current === controls.length - 1 ? 0 : current + 1);
  event.preventDefault();
  controls[next]?.focus();
}

function restoreBackground(): void {
  for (const [element, wasInert] of inertedElements) element.inert = wasInert;
  inertedElements.clear();
}
</script>

<template>
  <Teleport to="body">
    <Transition name="confirm">
      <div v-if="open" ref="backdrop" class="confirm-backdrop" @mousedown.self="cancel">
        <section
          ref="dialog"
          class="confirm-card"
          :class="tone"
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="confirm-title"
          aria-describedby="confirm-message"
          tabindex="-1"
          @keydown="handleKeydown"
        >
          <div class="signal-line"></div>
          <div class="confirm-content">
            <span class="confirm-icon"><AppIcon :name="tone === 'danger' ? 'trash' : 'warning'" :size="22" /></span>
            <div>
              <p>{{ eyebrow }}</p>
              <h2 id="confirm-title">{{ title }}</h2>
              <span id="confirm-message">{{ message }}</span>
            </div>
          </div>
          <footer>
            <button ref="cancelButton" type="button" :disabled="busy" @click="cancel">Cancelar</button>
            <button class="confirm-action" type="button" :disabled="busy" @click="emit('confirm')">
              <span v-if="busy" class="button-loader"></span>
              {{ busy ? "Procesando..." : confirmLabel }}
            </button>
          </footer>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.confirm-backdrop{position:fixed;inset:0;z-index:140;padding:20px;background:rgba(2,5,4,.78);backdrop-filter:blur(9px);display:grid;place-items:center}.confirm-card{position:relative;width:min(430px,100%);overflow:hidden;border:1px solid #343d38;border-radius:11px;background:linear-gradient(145deg,#111814,#0b100e 72%);box-shadow:0 28px 90px rgba(0,0,0,.62)}.signal-line{height:2px;background:#d3a24d;box-shadow:0 0 20px rgba(211,162,77,.28)}.confirm-card.danger .signal-line{background:var(--danger);box-shadow:0 0 20px rgba(220,119,112,.3)}.confirm-content{padding:25px 25px 22px;display:grid;grid-template-columns:44px 1fr;gap:15px}.confirm-icon{width:44px;height:44px;border:1px solid rgba(211,162,77,.22);border-radius:9px;background:rgba(211,162,77,.08);color:#d3a24d;display:grid;place-items:center}.danger .confirm-icon{border-color:rgba(220,119,112,.22);background:rgba(220,119,112,.08);color:var(--danger)}.confirm-content p{margin:1px 0 6px;color:#a17e42;font:8px var(--mono);letter-spacing:.15em}.danger .confirm-content p{color:#b86560}.confirm-content h2{margin:0;color:#e0e6e2;font-size:16px;letter-spacing:-.01em}.confirm-content div>span{display:block;margin-top:10px;color:#818b86;font-size:10px;line-height:1.65}.confirm-card footer{min-height:67px;padding:13px 17px;border-top:1px solid var(--line);background:rgba(6,10,8,.45);display:flex;align-items:center;justify-content:flex-end;gap:8px}.confirm-card footer button{min-width:95px;height:38px;padding:0 15px;border:1px solid #313a35;border-radius:6px;background:#111714;color:#8d9792;font-size:9px;font-weight:650;cursor:pointer}.confirm-card footer button:hover:not(:disabled){border-color:#4a554f;color:#d2dad6}.confirm-card footer .confirm-action{min-width:145px;border-color:rgba(211,162,77,.35);background:#d3a24d;color:#15130e}.danger footer .confirm-action{border-color:var(--danger);background:var(--danger);color:#190d0c}.confirm-card footer .confirm-action:hover:not(:disabled){filter:brightness(1.08);color:#15130e}.confirm-card footer button:disabled{opacity:.58}.button-loader{display:inline-block;width:11px;height:11px;margin-right:7px;border:1px solid currentColor;border-right-color:transparent;border-radius:50%;vertical-align:-2px;animation:confirm-spin .7s linear infinite}.confirm-enter-active,.confirm-leave-active{transition:opacity .16s ease}.confirm-enter-active .confirm-card,.confirm-leave-active .confirm-card{transition:transform .18s ease,opacity .16s ease}.confirm-enter-from,.confirm-leave-to{opacity:0}.confirm-enter-from .confirm-card,.confirm-leave-to .confirm-card{opacity:0;transform:translateY(8px) scale(.985)}@keyframes confirm-spin{to{transform:rotate(360deg)}}
@media(max-width:520px){.confirm-backdrop{padding:14px;align-items:end}.confirm-card{border-radius:11px}.confirm-content{padding:22px 19px 20px;grid-template-columns:40px 1fr;gap:12px}.confirm-icon{width:40px;height:40px}.confirm-card footer{padding:12px;display:grid;grid-template-columns:1fr 1.35fr}.confirm-card footer button,.confirm-card footer .confirm-action{min-width:0;width:100%}}
</style>
