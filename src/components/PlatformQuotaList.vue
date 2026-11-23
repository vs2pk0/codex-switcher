<script setup lang="ts">
import { computed } from "vue";
import { currentLocale, t } from "../i18n";
import { formatCreditBalance, quotaResetCountdown } from "../quota";
import { formatWallTime } from "../timeZone";
import type { PlatformQuota } from "../services/extensions";
import AccountBalance from "./AccountBalance.vue";
import AccountQuotaMetric from "./AccountQuotaMetric.vue";

const props = defineProps<{
  metrics: PlatformQuota[];
  balances: PlatformQuota[];
  statusClockMs: number;
  hideAmounts: boolean;
}>();

const quotaNumber = computed(() => new Intl.NumberFormat(currentLocale(), { maximumFractionDigits: 2 }));

function quotaPercent(quota: PlatformQuota): number | undefined {
  if (quota.unlimited || quota.remainingPercent === null) return undefined;
  return Math.round(quota.remainingPercent * 100) / 100;
}

function quotaAmount(quota: PlatformQuota): string | undefined {
  if (quota.unlimited) return t("不限量");
  const value = quota.balance ?? quota.used ?? quota.limit;
  if (value === null) return;
  if (props.hideAmounts) return "••••";
  const number = quotaNumber.value;
  let amount = number.format(value);
  if (quota.balance !== null) {
    amount = formatCreditBalance(quota.balance, currentLocale());
  } else if (quota.used !== null && quota.limit !== null) {
    amount = `${number.format(quota.used)} / ${number.format(quota.limit)}`;
  }
  return quota.unit ? `${amount} ${quota.unit}` : amount;
}

function quotaDetail(quota: PlatformQuota): string | undefined {
  if (quota.resetAt !== null) return t(`更新 ${formatWallTime(quota.resetAt)}`);
  return quotaPercent(quota) === undefined ? undefined : quotaAmount(quota);
}
</script>

<template>
  <div v-if="metrics.length" class="quota-panel">
    <div class="quota-metrics">
      <AccountQuotaMetric
        v-for="(quota, index) in metrics"
        :key="index"
        :label="t(quota.label)"
        :calendar="/[天周月]|day|week|month/i.test(quota.label)"
        :detail="quotaDetail(quota)"
        :countdown="quota.resetAt === null ? undefined : quotaResetCountdown(quota.resetAt, statusClockMs)"
        :percentage="quotaPercent(quota)"
        :amount="quotaAmount(quota)"
      />
    </div>
  </div>
  <AccountBalance
    v-for="(quota, index) in balances"
    :key="index"
    :label="t(quota.label)"
    :value="quotaAmount(quota) ?? '—'"
    :title="`${t(quota.label)} · ${quotaAmount(quota) ?? '—'}`"
  />
</template>
