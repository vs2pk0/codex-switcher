<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { currentLocale, formatLocalizedCount, t } from "../i18n";
import { effectiveTimeZone, formatWallTime } from "../timeZone";
import { extensionUsageBounds, type UsagePeriod } from "../services/extensionUsageTime";
import { extensionUsage, usageAppNames, type ExtensionUsage, type InstalledExtension, type UsageSyncResult } from "../services/extensions";
import { formatUsageTokens, normalizeTokenDisplayUnit, tokenDisplayStorageKey } from "../usageDisplay";
import UsageAppIcon from "./UsageAppIcon.vue";
import UsageSummaryCards from "./UsageSummaryCards.vue";
import UsageTrendChart from "./UsageTrendChart.vue";

const props = defineProps<{
  extension: InstalledExtension;
  appIcons: Record<string, string>;
  app: string;
  active: boolean;
  syncing: boolean;
  syncResult: UsageSyncResult | null;
  syncError: string;
}>();
const emit = defineEmits<{ sync: [] }>();
const periods: { key: UsagePeriod; label: string }[] = [
  { key: "today", label: "当天" },
  { key: "yesterday", label: "昨天" },
  { key: "thisMonth", label: "本月" },
  { key: "lastMonth", label: "上月" },
  { key: "custom", label: "自定义" },
];
const period = ref<UsagePeriod>("today");
const dates = ref<string[]>([]);
const provider = ref("");
const model = ref("");
const status = ref(0);
const page = ref(1);
const view = ref("requests");
const chartMetrics = [
  { key: "breakdown", label: "消耗明细" },
  { key: "tokens", label: "总 Tokens" },
  { key: "requests", label: "总请求数" },
  { key: "totalCost", label: "预估费用" },
  { key: "cacheHitRate", label: "缓存命中率" },
] as const;
const chartMetric = ref<(typeof chartMetrics)[number]["key"]>("breakdown");
const groupTabs = [
  { key: "providers", title: "来源汇总", label: "供应商", distribution: "来源分布", empty: "暂无来源数据" },
  { key: "models", title: "模型用量", label: "模型", distribution: "模型分布", empty: "暂无模型用量" },
] as const;
const tokenColumns = [
  { key: "input", title: "新增输入", width: 98 },
  { key: "output", title: "输出", width: 82 },
  { key: "cacheRead", title: "缓存命中", width: 100 },
  { key: "cacheWrite", title: "缓存写入", width: 100 },
] as const;
const tokenUnit = ref(normalizeTokenDisplayUnit(undefined));
const data = ref<ExtensionUsage | null>(null);
const loading = ref(false);
const queryError = ref("");
const hourly = ref(true);
let generation = 0;

const zone = effectiveTimeZone;
const numbers = computed(() => new Intl.NumberFormat(currentLocale()));
const percent = computed(() => new Intl.NumberFormat(currentLocale(), { style: "percent", maximumFractionDigits: 1 }));
const errors = computed(() => [...new Set([queryError.value, props.syncError].filter(Boolean))]);
const providerOptions = computed(() => [...new Set([...(data.value?.providerOptions ?? []), provider.value])].filter(Boolean));
const modelOptions = computed(() => [...new Set([...(data.value?.modelOptions ?? []), model.value])].filter(Boolean));
const statusOptions = computed(() => [...new Set([...(data.value?.statusOptions ?? []), status.value])].filter(Boolean));
const summary = computed(() => {
  const row = data.value?.summary;
  if (!row) return undefined;
  return {
    totalRequests: row.requests,
    realTotalTokens: row.tokens,
    totalInputTokens: row.input,
    totalOutputTokens: row.output,
    totalCacheReadTokens: row.cacheRead,
    totalCacheCreationTokens: row.cacheWrite,
    cacheHitRate: row.cacheHitRate,
  };
});
const trends = computed(() => {
  if (!data.value?.summary.requests) return [];
  return data.value.timeline.map((row) => {
    const caption = formatWallTime(row.start * 1000);
    return {
      caption,
      label: hourly.value ? caption.slice(11) : caption.slice(5, 10),
      inputTokens: row.input,
      outputTokens: row.output,
      cacheReadTokens: row.cacheRead,
      cacheCreationTokens: row.cacheWrite,
      totalCost: estimatedCost(row),
      tokens: row.tokens,
      requests: row.requests,
      cacheHitRate: row.cacheHitRate,
    };
  });
});

function sessionSource(id: string): string | undefined {
  if (id === "_session") return "claude";
  return id.match(/^_(.+?)(?:_account)?_session$/)?.[1] ?? id.match(/^_session_(.+)$/)?.[1];
}

function providerApps(id: string, app?: string): string[] {
  const source = sessionSource(id);
  if (!source) return [];
  if (app) return [app];
  const knownApps = data.value?.providerApplications?.[id];
  if (knownApps?.length) return knownApps;
  return [source === "grok" ? "grokbuild" : source];
}

function providerName(id: string, app?: string): string {
  const apps = providerApps(id, app);
  if (!apps.length) return id || t("会话日志");
  const names = apps.map((item) => usageAppNames[item] ?? item).join(" / ");
  const label = id.endsWith("_account_session") ? t("账号会话日志") : t("会话日志");
  return `${names} · ${label}`;
}

const providers = computed(() => providerOptions.value.map((id) => {
  const [app, other] = providerApps(id);
  return { id, label: providerName(id), app: other ? "" : app };
}));

const providerIcons = computed(() => Object.fromEntries(providers.value.map(({ id, app }) => [id, app])));

function providerIcon(id: string): string {
  return providerIcons.value[id] || "";
}

function money(value: number | null, digits = 2): string {
  return value === null ? t("未定价") : `$${value.toFixed(digits)}`;
}

function estimatedCost(row: ExtensionUsage["summary"]): number | null {
  return row.unpriced > 0 && row.unpriced === row.requests ? null : row.cost;
}

function tokens(value: number): string {
  return formatUsageTokens(value, tokenUnit.value, currentLocale());
}

function logTime(seconds: number): string {
  const formatted = formatWallTime(seconds * 1000, true);
  return hourly.value ? formatted.slice(11) : formatted.slice(5, 16);
}

async function load(): Promise<void> {
  const version = ++generation;
  if (!props.active) return;
  if (period.value === "custom" && dates.value.length !== 2) {
    data.value = null;
    loading.value = false;
    queryError.value = "";
    return;
  }
  loading.value = true;
  queryError.value = "";
  try {
    const { hourly: byHour, ...bounds } = extensionUsageBounds(period.value, dates.value, Date.now(), zone.value);
    const result = await extensionUsage(props.extension.id, {
      ...bounds,
      page: page.value,
      app: props.app || undefined,
      provider: provider.value || undefined,
      model: model.value || undefined,
      status: status.value || undefined,
    });
    if (version !== generation) return;
    const lastPage = Math.max(1, Math.ceil(result.summary.requests / result.pageSize));
    if (page.value > lastPage) {
      page.value = lastPage;
      return;
    }
    data.value = result;
    hourly.value = byHour;
  } catch (cause) {
    if (version === generation) {
      data.value = null;
      queryError.value = cause instanceof RangeError ? cause.message : String(cause);
    }
  } finally {
    if (version === generation) loading.value = false;
  }
}

watch(() => props.app, () => {
  provider.value = "";
  model.value = "";
  status.value = 0;
  data.value = null;
});

const filters = [period, dates, () => props.app, provider, model, status, zone] as const;
watch(filters, () => {
  page.value = 1;
});
watch([...filters, page, () => props.active, () => props.syncResult], () => {
  if (props.active) void load();
  else {
    generation += 1;
    loading.value = false;
  }
}, { flush: "post", immediate: true });
watch(() => props.active, (active) => {
  if (!active) return;
  try {
    tokenUnit.value = normalizeTokenDisplayUnit(window.localStorage.getItem(tokenDisplayStorageKey));
  } catch {
    tokenUnit.value = "compact";
  }
}, { immediate: true });
onBeforeUnmount(() => {
  generation += 1;
});
</script>

<template>
  <section class="usage-panel extension-usage">
    <div class="usage-head">
      <div>
        <h2>{{ app ? usageAppNames[app] ?? app : t(extension.name) }}</h2>
        <p v-if="syncResult">{{ t("最近同步") }} {{ formatWallTime(syncResult.completedAt) }}</p>
      </div>
      <div class="usage-controls">
        <a-select v-model="provider" :aria-label="t('供应商')" allow-search class="extension-filter extension-provider" size="mini">
          <template #prefix><icon-list /></template>
          <a-option value="">{{ t("全部供应商") }}</a-option>
          <a-option v-for="option in providers" :key="option.id" :value="option.id" :label="option.label">
            <span class="usage-app-cell">
              <UsageAppIcon v-if="option.app" :app="option.app" :icon-png="appIcons[option.app]" />
              {{ option.label }}
            </span>
          </a-option>
        </a-select>
        <a-select v-model="model" :aria-label="t('模型')" allow-search class="extension-filter" size="mini">
          <template #prefix><icon-code /></template>
          <a-option value="">{{ t("全部模型") }}</a-option>
          <a-option v-for="id in modelOptions" :key="id" :value="id">{{ id }}</a-option>
        </a-select>
        <a-radio-group v-model="period" type="button" size="mini" :aria-label="t('日期范围')">
          <a-radio v-for="item in periods" :key="item.key" :value="item.key">{{ t(item.label) }}</a-radio>
        </a-radio-group>
        <a-range-picker
          v-if="period === 'custom'"
          v-model="dates"
          :allow-clear="false"
          value-format="YYYY-MM-DD"
          :placeholder="[t('开始日期'), t('结束日期')]"
          class="usage-range-picker"
          size="mini"
        />
        <a-button size="mini" :loading="syncing" @click="emit('sync')">
          <template #icon><icon-refresh /></template>
          {{ t("同步本地用量") }}
        </a-button>
      </div>
    </div>

    <a-alert v-for="error in errors" :key="error" type="error">{{ t(error) }}</a-alert>
    <a-alert v-if="syncResult?.deferred || syncResult?.failed" type="warning">
      {{ t("部分日志待同步") }} {{ numbers.format(syncResult.deferred + syncResult.failed) }}
    </a-alert>
    <a-alert v-if="data?.summary.unpriced" type="info">
      {{ t("未定价的记录数") }} {{ numbers.format(data.summary.unpriced) }}
    </a-alert>

    <a-spin :loading="loading" dot class="usage-dashboard-spin">
      <div class="usage-dashboard-grid">
        <div class="usage-main-column">
          <UsageSummaryCards :summary="summary" :cost="data ? money(estimatedCost(data.summary)) : '—'" :unit="tokenUnit">
            <template #icon><UsageAppIcon :app="app" :icon-png="appIcons[app]" /></template>
          </UsageSummaryCards>
          <div class="usage-chart-card">
            <div class="usage-card-title">
              <strong>{{ t("时段消耗曲线") }}</strong>
              <a-select v-model="chartMetric" size="mini" class="extension-filter" :aria-label="t('时段消耗曲线')">
                <a-option v-for="item in chartMetrics" :key="item.key" :value="item.key">{{ t(item.label) }}</a-option>
              </a-select>
            </div>
            <UsageTrendChart :points="trends" :active="active" :unit="tokenUnit" :hourly="hourly" :metric="chartMetric" />
          </div>
        </div>

        <aside class="usage-side-column extension-usage-breakdown">
          <section v-for="group in groupTabs" :key="group.key" class="usage-side-card">
            <div class="usage-card-title">
              <strong>{{ t(group.distribution) }}</strong>
              <span>{{ formatLocalizedCount(data?.[group.key]?.length ?? 0, "个") }}</span>
            </div>
            <article v-for="row in data?.[group.key]?.slice(0, 5)" :key="row.key" class="usage-rank-row">
              <div>
                <strong v-if="group.key === 'providers'" class="usage-source-name">
                  <UsageAppIcon v-if="providerIcon(row.key)" :app="providerIcon(row.key)" :icon-png="appIcons[providerIcon(row.key)]" />
                  {{ providerName(row.key) }}
                </strong>
                <strong v-else>{{ row.key }}</strong>
                <span>{{ formatLocalizedCount(row.requests, "次") }} · {{ money(estimatedCost(row)) }}</span>
              </div>
              <b :title="numbers.format(row.tokens)">{{ tokens(row.tokens) }}</b>
            </article>
            <a-empty v-if="!data?.[group.key]?.length" :description="t(group.empty)" />
          </section>
        </aside>

        <section class="usage-table-card">
          <a-tabs v-model:active-key="view" class="usage-record-tabs" :lazy-load="true">
            <a-tab-pane key="requests" :title="t('请求日志')">
              <div class="usage-card-title">
                <span>{{ formatLocalizedCount(data?.summary.requests ?? 0, "条记录") }}</span>
                <a-select v-model="status" :aria-label="t('状态码')" class="extension-filter" size="mini">
                  <template #prefix><icon-filter /></template>
                  <a-option :value="0">{{ t("全部状态码") }}</a-option>
                  <a-option v-for="item in statusOptions" :key="item" :value="item">{{ item }}</a-option>
                </a-select>
              </div>
              <a-table :data="data?.requests ?? []" row-key="id" :pagination="false" :bordered="false" :scroll="{ x: 1380 }" class="usage-data-table">
                <template #columns>
                  <a-table-column :title="t('时间')" :width="125">
                    <template #cell="{ record }">
                      <span :title="formatWallTime(record.createdAt * 1000, true)">{{ logTime(record.createdAt) }}</span>
                    </template>
                  </a-table-column>
                  <a-table-column :title="t('应用')" :width="162">
                    <template #cell="{ record }">
                      <span class="usage-app-cell">
                        <UsageAppIcon :app="record.app" :icon-png="appIcons[record.app]" />
                        {{ usageAppNames[record.app] ?? record.app }}
                      </span>
                    </template>
                  </a-table-column>
                  <a-table-column :title="t('来源')" :width="194" ellipsis tooltip>
                    <template #cell="{ record }">{{ providerName(record.provider, record.app) }}</template>
                  </a-table-column>
                  <a-table-column :title="t('模型')" :width="170">
                    <template #cell="{ record }"><code class="usage-model-id" :title="record.model">{{ record.model }}</code></template>
                  </a-table-column>
                  <a-table-column :title="t('状态码')" align="right" :width="68">
                    <template #cell="{ record }">{{ record.status || "—" }}</template>
                  </a-table-column>
                  <a-table-column
                    v-for="column in tokenColumns"
                    :key="column.key"
                    :title="t(column.title)"
                    align="right"
                    :width="column.width"
                  >
                    <template #cell="{ record }">
                      <span :title="numbers.format(record[column.key])">{{ tokens(record[column.key]) }}</span>
                    </template>
                  </a-table-column>
                  <a-table-column :title="t('预估费用')" align="right" :width="110">
                    <template #cell="{ record }"><strong class="usage-cost">{{ money(record.cost, 4) }}</strong></template>
                  </a-table-column>
                  <a-table-column :title="t('速度')" align="right" :width="90">
                    <template #cell="{ record }">
                      <span class="usage-speed">{{ record.tokensPerSecond === null ? "—" : `≈${Math.round(record.tokensPerSecond)} tok/s` }}</span>
                    </template>
                  </a-table-column>
                  <a-table-column :title="t('首字延迟')" align="right" :width="90">
                    <template #cell="{ record }">{{ record.firstTokenMs === null ? "—" : `≈${(record.firstTokenMs / 1000).toFixed(1)} s` }}</template>
                  </a-table-column>
                </template>
                <template #empty><a-empty :description="t('暂无使用记录')" /></template>
              </a-table>
              <a-pagination
                v-if="data && data.summary.requests > data.pageSize"
                v-model:current="page"
                :total="data.summary.requests"
                :page-size="data.pageSize"
                show-total
                class="usage-pagination"
              />
            </a-tab-pane>

            <a-tab-pane v-for="group in groupTabs" :key="group.key" :title="t(group.title)">
              <a-table :data="data?.[group.key] ?? []" row-key="key" :pagination="false" :bordered="false" :scroll="{ x: 660 }" class="usage-data-table">
                <template #columns>
                  <a-table-column :title="t(group.label)" :width="220" ellipsis tooltip>
                    <template #cell="{ record }">
                      <span v-if="group.key === 'providers'" class="usage-app-cell">
                        <UsageAppIcon v-if="providerIcon(record.key)" :app="providerIcon(record.key)" :icon-png="appIcons[providerIcon(record.key)]" />
                        {{ providerName(record.key) }}
                      </span>
                      <template v-else>{{ record.key }}</template>
                    </template>
                  </a-table-column>
                  <a-table-column :title="t('总请求数')" align="right">
                    <template #cell="{ record }">{{ numbers.format(record.requests) }}</template>
                  </a-table-column>
                  <a-table-column :title="t('总 Tokens')" align="right">
                    <template #cell="{ record }"><span :title="numbers.format(record.tokens)">{{ tokens(record.tokens) }}</span></template>
                  </a-table-column>
                  <a-table-column :title="t('预估费用')" align="right">
                    <template #cell="{ record }"><strong class="usage-cost">{{ money(estimatedCost(record), 4) }}</strong></template>
                  </a-table-column>
                  <a-table-column :title="t('缓存命中率')" align="right">
                    <template #cell="{ record }">{{ record.cacheHitRate === null ? "—" : percent.format(record.cacheHitRate) }}</template>
                  </a-table-column>
                </template>
                <template #empty><a-empty :description="t(group.empty)" /></template>
              </a-table>
            </a-tab-pane>
          </a-tabs>
        </section>
      </div>
    </a-spin>
  </section>
</template>

<style scoped>
.extension-usage .usage-controls { flex-basis: 500px; }
.extension-usage :deep(.extension-filter) { width: 146px; }
.extension-usage :deep(.extension-provider) { width: 205px; }
.extension-usage > .arco-alert { margin-bottom: 10px; }
.usage-rank-row .usage-source-name {
  display: flex;
  align-items: center;
  gap: 6px;
}
@media (min-width: 901px) and (max-width: 1350px) {
  .extension-usage-breakdown { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
