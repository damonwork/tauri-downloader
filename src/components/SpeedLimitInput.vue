<script setup lang="ts">
import { computed, ref } from "vue";
import { formatSpeed } from "@/domain/format";
import {
  SPEED_UNITS,
  isSpeedUnit,
  maxAmountForUnit,
  speedLimitBytes,
  speedLimitInput,
  type SpeedUnit,
} from "@/domain/speed-limit";

defineProps<{ name: string; label?: string }>();
const model = defineModel<number>({ required: true });
const initial = speedLimitInput(model.value);
const amount = ref<number | undefined>(initial.amount);
const unit = ref<SpeedUnit>(initial.unit);
const amountInput = ref<HTMLInputElement>();
const maxAmount = computed(() => maxAmountForUnit(unit.value));
const summary = computed(() =>
  model.value === 0 ? "Sin límite" : `Tope: ${formatSpeed(model.value)}`,
);

function updateAmount(event: Event): void {
  const value = (event.target as HTMLInputElement).valueAsNumber;
  if (!Number.isFinite(value)) {
    amount.value = undefined;
    model.value = 0;
    return;
  }
  amount.value = Math.min(maxAmount.value, Math.max(0, value));
  if (value !== amount.value) (event.target as HTMLInputElement).value = String(amount.value);
  model.value = speedLimitBytes(amount.value, unit.value);
}

function updateUnit(event: Event): void {
  const value = (event.target as HTMLSelectElement).value;
  if (!isSpeedUnit(value)) return;
  unit.value = value;
  amount.value = Math.min(maxAmountForUnit(value), amount.value ?? 0);
  model.value = speedLimitBytes(amount.value, value);
}

function normalizeAmount(): void {
  if (amount.value !== undefined) return;
  amount.value = 0;
  if (amountInput.value) amountInput.value.value = "0";
}
</script>

<template>
  <div class="speed-limit-input w-full min-w-0">
    <div
      class="speed-fields grid h-10 grid-cols-[minmax(0,1fr)_78px] overflow-hidden rounded-lg border border-line bg-canvas [transition:border-color_.15s] focus-within:border-muted focus-within:shadow-[0_0_0_1px_rgba(194,255,91,.06)]"
    >
      <input
        class="h-full w-full min-w-0 border-0 bg-transparent px-2.5 py-0 font-mono text-foreground outline-none [&::-webkit-inner-spin-button]:opacity-35"
        ref="amountInput"
        :id="`${name}-amount`"
        :name="`${name}-amount`"
        :value="amount ?? ''"
        :aria-label="label ?? 'Límite de velocidad'"
        type="number"
        inputmode="decimal"
        min="0"
        :max="maxAmount"
        step="any"
        placeholder="0"
        @input="updateAmount"
        @blur="normalizeAmount"
      />
      <select
        class="h-full min-w-0 cursor-pointer border-0 border-l border-l-line bg-transparent px-[7px] py-0 font-mono text-accent outline-none"
        :name="`${name}-unit`"
        :value="unit"
        aria-label="Unidad del límite de velocidad"
        @change="updateUnit"
      >
        <option
          class="bg-raised text-foreground"
          v-for="speedUnit in SPEED_UNITS"
          :key="speedUnit"
          :value="speedUnit"
        >
          {{ speedUnit }}/s
        </option>
      </select>
    </div>
    <small
      class="mt-[5px] block font-mono text-muted [&.unlimited]:text-muted"
      :class="{ unlimited: model === 0 }"
      >{{ summary }} · 0 desactiva el límite</small
    >
  </div>
</template>
