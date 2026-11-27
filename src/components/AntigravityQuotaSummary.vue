<script setup lang="ts">
import { t } from "../i18n";
import { type antigravityQuotaGroups, quotaResetCountdown } from "../quota";
import { formatWallTime } from "../timeZone";
import AccountQuotaMetric from "./AccountQuotaMetric.vue";

defineProps<{
  groups: ReturnType<typeof antigravityQuotaGroups>;
  statusClockMs: number;
}>();
</script>

<template>
  <div class="quota-panel">
    <section v-for="group in groups" :key="group.label" class="quota-metrics antigravity-quota-group">
      <h3>{{ t(group.label) }}</h3>
      <AccountQuotaMetric
        v-for="quota in group.windows"
        :key="quota.id"
        :label="t(quota.label)"
        :calendar="quota.label === '每周额度'"
        :detail="quota.resetAt === null ? undefined : t(`更新 ${formatWallTime(quota.resetAt)}`)"
        :countdown="quota.resetAt === null ? undefined : quotaResetCountdown(quota.resetAt, statusClockMs)"
        :percentage="Math.round(quota.percentage * 100) / 100"
      />
    </section>
  </div>
</template>

<style scoped>
.antigravity-quota-group {
  grid-template-rows: auto;
}
.antigravity-quota-group h3 {
  margin: 0;
  padding: 7px 12px;
  border-bottom: 1px solid var(--line);
  color: var(--text-soft);
  font-size: 13px;
  font-weight: 720;
}
</style>
