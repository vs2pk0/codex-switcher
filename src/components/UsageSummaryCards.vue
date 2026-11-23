<script setup lang="ts">
import { computed } from "vue";
import { currentLocale, formatLocalizedNumber, t } from "../i18n";
import type { CodexUsageSummary } from "../services/usage";
import { formatUsageTokens, type TokenDisplayUnit } from "../usageDisplay";

const props = defineProps<{
  summary?: Omit<CodexUsageSummary, "totalCost" | "cacheHitRate"> & { cacheHitRate: number | null };
  cost: string;
  unit: TokenDisplayUnit;
}>();
const hitRate = computed(() => props.summary ? props.summary.cacheHitRate : 0);
const percent = computed(() => hitRate.value == null ? "—" : `${(hitRate.value * 100).toFixed(1)}%`);

function tokens(value?: number): string {
  return formatUsageTokens(value, props.unit, currentLocale());
}
</script>

<template>
  <div class="usage-hero">
    <div class="usage-hero-main">
      <div class="usage-app-mark"><slot name="icon" /></div>
      <div>
        <div class="usage-token-heading">
          <span>{{ t("真实消耗 Tokens") }}</span>
        </div>
        <strong :title="formatLocalizedNumber(summary?.realTotalTokens)">{{ tokens(summary?.realTotalTokens) }}</strong>
      </div>
    </div>
    <div class="usage-hero-side">
      <div>
        <span>{{ t("总请求数") }}</span>
        <strong>{{ formatLocalizedNumber(summary?.totalRequests) }}</strong>
      </div>
      <div>
        <span>{{ t("预估费用") }}</span>
        <strong>{{ cost }}</strong>
      </div>
    </div>
  </div>

  <div class="usage-metrics">
    <article>
      <span><icon-arrow-down /> {{ t("新增输入 Tokens") }}</span>
      <strong>{{ tokens(summary?.totalInputTokens) }}</strong>
    </article>
    <article>
      <span><icon-arrow-up /> {{ t("输出 Tokens") }}</span>
      <strong>{{ tokens(summary?.totalOutputTokens) }}</strong>
    </article>
    <article>
      <span><icon-storage /> {{ t("缓存写入") }}</span>
      <strong>{{ tokens(summary?.totalCacheCreationTokens) }}</strong>
    </article>
    <article>
      <span><icon-thunderbolt /> {{ t("缓存复用") }}</span>
      <strong>{{ tokens(summary?.totalCacheReadTokens) }}</strong>
    </article>
    <article>
      <span>{{ t("复用占比") }}</span>
      <strong>{{ percent }}</strong>
      <div class="usage-hit-bar">
        <i :style="{ width: hitRate == null ? '0%' : percent }" />
      </div>
    </article>
  </div>
</template>
