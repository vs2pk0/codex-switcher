<script setup lang="ts">
import { computed } from "vue";
import { creditBalance, formatCreditBalance } from "../quota";
import { currentLocale, t } from "../i18n";
import type { CodexQuota } from "../types/codex";
import AccountBalance from "./AccountBalance.vue";

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
  <AccountBalance :label="t('积分余额')" :value="value" :inline="inline" :title="title" />
</template>
