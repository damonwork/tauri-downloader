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
const summary = computed(() => model.value === 0 ? "Sin límite" : `Tope: ${formatSpeed(model.value)}`);

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
  <div class="speed-limit-input">
    <div class="speed-fields">
      <input
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
      <select :name="`${name}-unit`" :value="unit" aria-label="Unidad del límite de velocidad" @change="updateUnit">
        <option v-for="speedUnit in SPEED_UNITS" :key="speedUnit" :value="speedUnit">{{ speedUnit }}/s</option>
      </select>
    </div>
    <small :class="{ unlimited: model === 0 }">{{ summary }} · 0 desactiva el límite</small>
  </div>
</template>

<style scoped>
.speed-limit-input{width:100%;min-width:0}.speed-fields{height:36px;display:grid;grid-template-columns:minmax(0,1fr) 78px;border:1px solid #2c3631;border-radius:6px;background:#0a0f0d;overflow:hidden;transition:border-color .15s}.speed-fields:focus-within{border-color:#536348;box-shadow:0 0 0 1px rgba(194,255,91,.06)}.speed-fields input,.speed-fields select{min-width:0;height:100%;border:0;background:transparent;color:#cbd3cf;outline:0;font:10px var(--mono)}.speed-fields input{width:100%;padding:0 10px}.speed-fields select{padding:0 7px;border-left:1px solid #29322e;color:var(--accent);cursor:pointer}.speed-fields select option{background:#111714;color:#cbd3cf}.speed-limit-input>small{display:block;margin-top:5px;color:#68736d;font:8px var(--mono)}.speed-limit-input>small.unlimited{color:#59635e}input::-webkit-inner-spin-button{opacity:.35}
</style>
