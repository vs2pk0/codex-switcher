<script setup lang="ts">
import { nextTick } from "vue";
import { effectiveTimeZone } from "../timeZone";

const props = defineProps<{ modelValue?: string; label: string; seconds?: boolean; disabled?: boolean }>();
const emit = defineEmits<{ (event: "update:modelValue", value: string): void }>();
function inputValue(value?: string): string {
  return value?.replace(" ", "T").replaceAll("/", "-") || "";
}
async function change(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement;
  const value = input.value.replace("T", " ");
  emit("update:modelValue", props.seconds && value.length === 16 ? `${value}:00` : value);
  await nextTick();
  // Restore the accepted value if the parent rejected an invalid date range.
  input.value = inputValue(props.modelValue);
}
</script>

<template>
  <!-- Keep wall-clock fields as text: Date-based pickers can normalize them in the host's DST zone. -->
  <input
    :key="effectiveTimeZone"
    type="datetime-local"
    class="time-zone-date-time-input"
    :aria-label="`${label} (${effectiveTimeZone})`"
    :title="effectiveTimeZone"
    :value="inputValue(modelValue)"
    :step="seconds ? 1 : 60"
    :disabled="disabled"
    @change="change"
  />
</template>

<style scoped>
.time-zone-date-time-input { box-sizing: border-box; min-width: 0; height: 32px; padding: 4px 10px; border: 1px solid transparent; border-radius: 4px; background: var(--color-fill-2, #f2f3f5); color: var(--color-text-1, #1d2129); font: inherit; font-size: 13px; }
.time-zone-date-time-input:focus { outline: none; border-color: rgb(var(--primary-6, 22, 93, 255)); background: var(--color-bg-2, white); }
.time-zone-date-time-input:disabled { cursor: not-allowed; opacity: .6; }
</style>
