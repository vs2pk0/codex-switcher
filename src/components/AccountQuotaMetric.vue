<script setup lang="ts">
import { computed } from "vue";
import { quotaColor, quotaProgressStyle } from "../quota";

const props = defineProps<{
  label: string;
  calendar?: boolean;
  detail?: string;
  countdown?: string;
  percentage?: number;
  amount?: string;
}>();
const hasPercentage = computed(() => Number.isFinite(props.percentage));
</script>

<template>
  <div class="quota-metric">
    <div class="quota-metric-main">
      <span class="quota-window-label" :title="label">
        <icon-calendar v-if="calendar" />
        <icon-clock-circle v-else />
        <span>{{ label }}</span>
      </span>
      <em v-if="detail" :title="detail">{{ detail }}</em>
      <small v-if="countdown">{{ countdown }}</small>
      <strong
        :class="{ 'quota-amount': !hasPercentage }"
        :style="hasPercentage ? { color: quotaColor(percentage) } : undefined"
        :title="amount"
      >
        {{ hasPercentage ? `${percentage}%` : amount ?? "--" }}
      </strong>
    </div>
    <div v-if="hasPercentage" class="quota-bar" aria-hidden="true">
      <span :style="quotaProgressStyle(percentage)" />
    </div>
  </div>
</template>

<style scoped>
.quota-window-label > svg {
  flex: 0 0 auto;
}
.quota-amount {
  max-width: 8em;
  overflow: hidden;
  color: #526176;
  font-variant-numeric: tabular-nums;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
