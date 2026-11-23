<script setup lang="ts">
defineProps<{
  label: string;
  value: string;
  inline?: boolean;
}>();
</script>

<template>
  <span class="codex-credit-balance" :class="{ 'is-inline': inline }">
    <span class="credit-label"><icon-safe v-if="!inline" /><span>{{ label }}</span></span>
    <strong>{{ value }}</strong>
  </span>
</template>

<style scoped>
.codex-credit-balance {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 38px;
  margin-top: 8px;
  padding: 8px 12px;
  border: 1px solid rgba(13, 148, 136, 0.16);
  border-radius: 8px;
  background: rgba(240, 253, 250, 0.7);
  color: #0f766e;
}
.credit-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  font-size: 12px;
  font-weight: 650;
  white-space: nowrap;
}
.credit-label > svg { flex: 0 0 auto; }
.credit-label > span { overflow: hidden; text-overflow: ellipsis; }
.codex-credit-balance strong {
  font-size: 15px;
  font-weight: 750;
  font-variant-numeric: tabular-nums;
  overflow-wrap: anywhere;
}
.codex-credit-balance.is-inline {
  display: inline-flex;
  justify-content: flex-start;
  gap: 6px;
  min-height: 0;
  margin: 0;
  padding: 0;
  border: 0;
  background: transparent;
}
.is-inline .credit-label, .is-inline strong { font-size: 12px; }
</style>
