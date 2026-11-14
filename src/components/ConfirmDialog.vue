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

watch(
  () => props.open,
  async (open) => {
    if (open) {
      previousFocus.value =
        document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
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
  },
);

watch(
  () => props.busy,
  async (busy) => {
    if (!busy || !props.open) return;
    await nextTick();
    dialog.value?.focus();
  },
);

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
  const controls = Array.from(
    dialog.value?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [],
  );
  if (controls.length === 0) {
    event.preventDefault();
    dialog.value?.focus();
    return;
  }
  const current = controls.indexOf(document.activeElement as HTMLButtonElement);
  const next = event.shiftKey
    ? current <= 0
      ? controls.length - 1
      : current - 1
    : current === controls.length - 1
      ? 0
      : current + 1;
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
      <div
        v-if="open"
        ref="backdrop"
        class="confirm-backdrop fixed inset-0 z-140 grid place-items-center bg-[rgba(2,5,4,.78)] p-5 backdrop-blur-[9px] max-narrow:items-end max-narrow:p-3.5"
        @mousedown.self="cancel"
      >
        <section
          ref="dialog"
          class="confirm-card relative w-[min(430px,100%)] overflow-hidden rounded-xl border border-line bg-[linear-gradient(145deg,#111814,#0b100e_72%)] shadow-[0_28px_90px_rgba(0,0,0,.62)] max-narrow:rounded-xl"
          :class="tone"
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="confirm-title"
          aria-describedby="confirm-message"
          tabindex="-1"
          @keydown="handleKeydown"
        >
          <div
            class="signal-line h-0.5 bg-warning shadow-[0_0_20px_rgba(211,162,77,.28)] [.confirm-card.danger_&]:bg-danger [.confirm-card.danger_&]:shadow-[0_0_20px_rgba(220,119,112,.3)]"
          ></div>
          <div
            class="confirm-content grid grid-cols-[44px_1fr] gap-[15px] [padding:25px_25px_22px] max-narrow:grid-cols-[40px_1fr] max-narrow:gap-3 max-narrow:[padding:22px_19px_20px]"
          >
            <span
              class="confirm-icon grid h-11 w-11 place-items-center rounded-xl border border-[rgba(211,162,77,.22)] bg-[rgba(211,162,77,.08)] text-warning max-narrow:h-10 max-narrow:w-10 [.danger_&]:border-[rgba(220,119,112,.22)] [.danger_&]:bg-[rgba(220,119,112,.08)] [.danger_&]:text-danger"
              ><AppIcon :name="tone === 'danger' ? 'trash' : 'warning'" :size="22"
            /></span>
            <div>
              <p
                class="[margin:1px_0_6px] font-mono tracking-[.15em] text-warning [.danger_.confirm-content_&]:text-danger"
              >
                {{ eyebrow }}
              </p>
              <h2 class="m-0 text-base tracking-[-.01em] text-foreground" id="confirm-title">
                {{ title }}
              </h2>
              <span class="mt-2.5 block leading-[1.65] text-muted" id="confirm-message">{{
                message
              }}</span>
            </div>
          </div>
          <footer
            class="flex min-h-[67px] items-center justify-end gap-2 border-t border-t-line bg-[rgba(6,10,8,.45)] px-[17px] py-[13px] max-narrow:grid max-narrow:grid-cols-[1fr_1.35fr] max-narrow:p-3"
          >
            <button
              class="h-9.5 min-w-[95px] cursor-pointer rounded-lg border border-line bg-raised px-[15px] py-0 font-[650] text-muted disabled:opacity-58 max-narrow:w-full max-narrow:min-w-0 [&:hover:not(:disabled)]:border-muted [&:hover:not(:disabled)]:text-foreground"
              ref="cancelButton"
              type="button"
              :disabled="busy"
              @click="cancel"
            >
              Cancelar
            </button>
            <button
              class="confirm-action h-9.5 min-w-[145px] cursor-pointer rounded-lg border border-[rgba(211,162,77,.35)] bg-warning px-[15px] py-0 font-[650] text-surface disabled:opacity-58 max-narrow:w-full max-narrow:min-w-0 [&:hover:not(:disabled)]:border-muted [&:hover:not(:disabled)]:text-surface [&:hover:not(:disabled)]:[filter:brightness(1.08)] [.danger_footer_&]:border-danger [.danger_footer_&]:bg-danger [.danger_footer_&]:text-surface"
              type="button"
              :disabled="busy"
              @click="emit('confirm')"
            >
              <span
                v-if="busy"
                class="button-loader mr-[7px] inline-block h-[11px] w-[11px] animate-[fluxor-spin_.7s_linear_infinite] rounded-full border border-[currentColor] [border-right-color:transparent] [vertical-align:-2px]"
              ></span>
              {{ busy ? "Procesando..." : confirmLabel }}
            </button>
          </footer>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>
