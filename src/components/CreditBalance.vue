<script setup lang="ts">
import { computed } from "vue";
import { creditBalance, formatCreditBalance } from "../quota";
import { currentLocale, t } from "../i18n";
import type { CodexQuota } from "../types/codex";

const props = defineProps<{
  quota?: CodexQuota;
  privacyMasked: boolean;
  inline?: boolean;
}>();
const credits = computed(() => creditBalance(props.quota));
const value = computed(() => {
  if (props.privacyMasked) return "••••";
  if (credits.value?.unlimited) return t("无限额度");
  return credits.value?.balance === undefined ? "--" : formatCreditBalance(credits.value.balance, currentLocale());
});
const title = computed(() => {
  if (props.privacyMasked) return t("积分余额已隐藏");
  if (credits.value?.unlimited) return `${t("积分余额")}：${t("无限额度")}`;
  if (credits.value?.balance === undefined) return t("未返回积分余额，可刷新额度重试");
  return `${t("积分余额")}：${credits.value.rawBalance}`;
});
</script>

<template>
  <span class="codex-credit-balance" :class="{ 'is-inline': inline }" :title="title">
    <span class="credit-label"><icon-safe v-if="!inline" />{{ t("积分余额") }}</span>
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
.credit-label { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 650; white-space: nowrap; }
.codex-credit-balance strong { font-size: 15px; font-weight: 750; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
.codex-credit-balance.is-inline { display: inline-flex; justify-content: flex-start; gap: 6px; min-height: 0; margin: 0; padding: 0; border: 0; background: transparent; }
.is-inline .credit-label, .is-inline strong { font-size: 12px; }
</style>
