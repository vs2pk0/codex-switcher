<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, shallowRef, watch } from "vue";
import { Message } from "@arco-design/web-vue";
import type { ECharts, EChartsCoreOption } from "echarts/core";
import {
  deleteModelPricing,
  getCodexUsageActivity,
  getPricingConfig,
  getCodexUsageDashboard,
  listModelPricing,
  resetModelPricing,
  updatePricingConfig,
  updateModelPricing,
  type CodexUsageActivity,
  type CodexUsageActivityDay,
  type CodexUsageActivityHour,
  type CodexUsageDashboard,
  type CodexUsagePricing,
  type CodexUsagePricingConfig,
} from "../services/usage";
import {
  currentLanguage,
  currentLocale,
  formatCompactTokens,
  formatLocalizedCount,
  formatLocalizedNumber,
  t,
} from "../i18n";

type UsageRange = "today" | "yesterday" | "beforeYesterday" | "thisMonth" | "lastMonth" | "custom";
type UsageTooltipParam = {
  seriesName?: string;
  marker?: string;
  value?: number | Array<string | number>;
};
type EChartsCoreApi = typeof import("echarts/core");
type ActivityMode = "daily" | "weekly" | "cumulative";
type ActivityCell = {
  key: string;
  label: string;
  timestamp: number;
  tokens: number;
  requests: number;
  value: number;
  level: number;
};
type ActivityBlock = ActivityCell & {
  subtitle: string;
  detail: string;
  percent: number;
};
type AutoRefreshOption = {
  value: number;
  label: string;
};

const props = withDefaults(defineProps<{
  active?: boolean;
}>(), {
  active: true,
});

const pageSize = 20;
const loading = ref(false);
const pricingLoading = ref(false);
const savingPricing = ref(false);
const savingPricingConfig = ref(false);
const pricingCollapsed = ref(true);
const pricingLoaded = ref(false);
const pricingModalVisible = ref(false);
const activeUsageTab = ref<"logs" | "providers" | "models">("logs");
const range = ref<UsageRange>("today");
const page = ref(1);
const chartContainer = ref<HTMLDivElement | null>(null);
const usageChart = shallowRef<ECharts | null>(null);
const dateRange = ref<[string, string] | undefined>();
const dashboard = ref<CodexUsageDashboard | null>(null);
const activity = ref<CodexUsageActivity | null>(null);
const activityMode = ref<ActivityMode>("daily");
const pricingList = ref<CodexUsagePricing[]>([]);
const editingModelId = ref<string | null>(null);
const pricingConfigs = ref<CodexUsagePricingConfig[]>([]);
const originalPricingConfigs = ref<CodexUsagePricingConfig[]>([]);
const lastLoadedAt = ref(0);
const pricingForm = reactive<CodexUsagePricing>({
  modelId: "",
  displayName: "",
  inputCostPerMillion: "0",
  outputCostPerMillion: "0",
  cacheReadCostPerMillion: "0",
  cacheCreationCostPerMillion: "0",
});

const rangeOptions: Array<{ value: UsageRange; label: string }> = [
  { value: "today", label: "当天" },
  { value: "yesterday", label: "昨天" },
  { value: "beforeYesterday", label: "前天" },
  { value: "thisMonth", label: "当月" },
  { value: "lastMonth", label: "上月" },
];
const autoRefreshStorageKey = "codex-switcher:usage-auto-refresh-ms";
const autoRefreshOptions: AutoRefreshOption[] = [
  { value: 0, label: "关闭" },
  { value: 5_000, label: "5s" },
  { value: 10_000, label: "10s" },
  { value: 30_000, label: "30s" },
  { value: 60_000, label: "60s" },
];
const autoRefreshIntervalMs = ref(readAutoRefreshInterval());

const summary = computed(() => dashboard.value?.summary);
const logs = computed(() => dashboard.value?.logs ?? []);
const trends = computed(() => dashboard.value?.trends ?? []);
const activitySummary = computed(() => activity.value?.summary);
const activityDays = computed(() => activity.value?.days ?? []);
const activityHours = computed(() =>
  activity.value?.hours?.length
    ? activity.value.hours
    : Array.from({ length: 24 }, (_, hour) => ({
        hour,
        label: `${String(hour).padStart(2, "0")}:00`,
        timestamp: 0,
        tokens: 0,
        requests: 0,
      })),
);
const providerStats = computed(() => dashboard.value?.providerStats ?? []);
const modelStats = computed(() => dashboard.value?.modelStats ?? []);
const totalLogs = computed(() => dashboard.value?.totalLogs ?? 0);
const topProvider = computed(() => providerStats.value[0]);
const topModel = computed(() => modelStats.value[0]);
const codexPricingConfig = computed(
  () =>
    pricingConfigs.value.find((item) => item.app.toLowerCase() === "codex") ?? {
      app: "Codex",
      multiplier: "1",
      pricingModelSource: "response",
    },
);
const totalPages = computed(() => Math.max(1, Math.ceil(totalLogs.value / pageSize)));
const pricingConfigDirty = computed(
  () => JSON.stringify(pricingConfigs.value) !== JSON.stringify(originalPricingConfigs.value),
);
const isHourlyTrend = computed(() =>
  trends.value.length > 1
    ? trends.value[1].timestamp - trends.value[0].timestamp <= 60 * 60
    : range.value === "today",
);
const recentActivityDays = computed(() => activityDays.value.slice(-7));
const activityWeeklyTotals = computed(() => {
  return recentActivityDays.value.map((day) => day.tokens);
});
const activityCells = computed<ActivityCell[]>(() => {
  let cumulative = 0;
  const source =
    activityMode.value === "daily"
      ? activityHours.value.map(activityHourToCellSource)
      : activityMode.value === "weekly"
        ? recentActivityDays.value.map(activityDayToCellSource)
        : activityDays.value.map(activityDayToCellSource);
  const values = source.map((cell, index) => {
    if (activityMode.value === "cumulative") {
      cumulative += cell.tokens;
      return cumulative;
    }
    if (activityMode.value === "weekly") return activityWeeklyTotals.value[index] ?? 0;
    return cell.tokens;
  });
  const max = Math.max(...values, 0);
  return source.map((cell, index) => ({
    ...cell,
    value: values[index] ?? 0,
    level: activityLevel(values[index] ?? 0, max),
  }));
});
const activityColumnCount = computed(() => {
  if (activityMode.value === "daily") return 24;
  if (activityMode.value === "weekly") return 7;
  return Math.max(1, Math.ceil(activityDays.value.length / 7));
});
const activityRowCount = computed(() => (activityMode.value === "cumulative" ? 7 : 1));
const activityLabels = computed(() => {
  if (activityMode.value === "daily") {
    return [0, 6, 12, 18, 23].map((hour) => ({
      key: `hour-${hour}`,
      label: `${String(hour).padStart(2, "0")}:00`,
      column: hour + 1,
    }));
  }
  if (activityMode.value === "weekly") {
    return recentActivityDays.value.map((day, index) => ({
      key: day.date,
      label: activityWeekdayLabel(day),
      column: index + 1,
    }));
  }
  const labels: Array<{ key: string; label: string; column: number }> = [];
  let previousMonth = "";
  activityDays.value.forEach((day, index) => {
    const currentMonth = day.date.slice(5, 7);
    if (currentMonth !== previousMonth) {
      const nextLabel = {
        key: day.date,
        label: new Intl.DateTimeFormat(currentLocale(), { month: "short" }).format(new Date(day.timestamp * 1000)),
        column: Math.floor(index / 7) + 1,
      };
      const previous = labels[labels.length - 1];
      if (previous && nextLabel.column - previous.column < 4) {
        labels[labels.length - 1] = nextLabel;
      } else {
        labels.push(nextLabel);
      }
      previousMonth = currentMonth;
    }
  });
  return labels;
});
const dailyActivityBlocks = computed<ActivityBlock[]>(() => {
  const segments = [
    { start: 0, end: 3, label: "凌晨", subtitle: "00:00-03:59" },
    { start: 4, end: 7, label: "清晨", subtitle: "04:00-07:59" },
    { start: 8, end: 11, label: "上午", subtitle: "08:00-11:59" },
    { start: 12, end: 15, label: "午后", subtitle: "12:00-15:59" },
    { start: 16, end: 19, label: "傍晚", subtitle: "16:00-19:59" },
    { start: 20, end: 23, label: "夜间", subtitle: "20:00-23:59" },
  ];
  const rows = segments.map((segment) => {
    const hours = activityHours.value.slice(segment.start, segment.end + 1);
    const tokens = hours.reduce((sum, item) => sum + item.tokens, 0);
    const requests = hours.reduce((sum, item) => sum + item.requests, 0);
    return {
      key: `daily-${segment.start}`,
      label: segment.label,
      subtitle: segment.subtitle,
      detail: `${formatFullNumber(tokens)} Tokens · ${requests} 次`,
      timestamp: hours[0]?.timestamp ?? 0,
      tokens,
      requests,
      value: tokens,
      level: 0,
      percent: 0,
    };
  });
  const max = Math.max(...rows.map((item) => item.value), 0);
  return rows.map((item) => ({
    ...item,
    level: activityLevel(item.value, max),
    percent: max > 0 ? Math.max(6, Math.round((item.value / max) * 100)) : 0,
  }));
});
const weeklyActivityBlocks = computed<ActivityBlock[]>(() => {
  const max = Math.max(...recentActivityDays.value.map((day) => day.tokens), 0);
  return recentActivityDays.value.map((day) => {
    const value = day.tokens;
    return {
      ...activityDayToCellSource(day),
      label: activityWeekdayLabel(day),
      subtitle: day.date,
      detail: `${formatFullNumber(value)} Tokens · ${day.requests} 次`,
      value,
      level: activityLevel(value, max),
      percent: max > 0 ? Math.max(8, Math.round((value / max) * 100)) : 0,
    };
  });
});
const activityModeOptions: Array<{ value: ActivityMode; label: string }> = [
  { value: "daily", label: "每日" },
  { value: "weekly", label: "每周" },
  { value: "cumulative", label: "累计" },
];
let chartResizeObserver: ResizeObserver | null = null;
let echartsApi: EChartsCoreApi | null = null;
let echartsLoading: Promise<EChartsCoreApi> | null = null;
let autoRefreshTimer: number | undefined;
let activationRefreshTimer: number | undefined;
let loadSerial = 0;

function readAutoRefreshInterval(): number {
  const fallback = 30_000;
  try {
    const stored = window.localStorage.getItem(autoRefreshStorageKey);
    const parsed = Number(stored);
    return autoRefreshOptions.some((item) => item.value === parsed) ? parsed : fallback;
  } catch {
    return fallback;
  }
}

function errorText(error: unknown): string {
  return String(error instanceof Error ? error.message : error).replace(/^Error:\s*/, "");
}

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

function formatPickerValue(date: Date): string {
  return `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate(), 0, 0, 0, 0);
}

function endOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate(), 23, 59, 59, 999);
}

function startOfMonth(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), 1, 0, 0, 0, 0);
}

function endOfMonth(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth() + 1, 0, 23, 59, 59, 999);
}

function nextWholeHour(date: Date): Date {
  const next = new Date(date);
  next.setMinutes(0, 0, 0);
  next.setHours(next.getHours() + 1);
  return next;
}

function seconds(date: Date): number {
  return Math.floor(date.getTime() / 1000);
}

function presetDates(preset: UsageRange): { start: Date; end: Date } {
  const now = new Date();
  if (preset === "yesterday") {
    const date = new Date(now);
    date.setDate(date.getDate() - 1);
    return { start: startOfDay(date), end: endOfDay(date) };
  }
  if (preset === "beforeYesterday") {
    const date = new Date(now);
    date.setDate(date.getDate() - 2);
    return { start: startOfDay(date), end: endOfDay(date) };
  }
  if (preset === "thisMonth") {
    return { start: startOfMonth(now), end: now };
  }
  if (preset === "lastMonth") {
    const date = new Date(now.getFullYear(), now.getMonth() - 1, 1);
    return { start: startOfMonth(date), end: endOfMonth(date) };
  }
  return { start: startOfDay(now), end: nextWholeHour(now) };
}

function syncPickerToPreset(preset: UsageRange): void {
  const { start, end } = presetDates(preset);
  dateRange.value = [formatPickerValue(start), formatPickerValue(end)];
}

function parsePickerDate(value: unknown): Date | null {
  if (value instanceof Date) return value;
  if (typeof value === "number") {
    return new Date(value > 10_000_000_000 ? value : value * 1000);
  }
  if (typeof value === "string" && value.trim()) {
    const parsed = new Date(value.replace(/-/g, "/"));
    return Number.isNaN(parsed.getTime()) ? null : parsed;
  }
  return null;
}

function normalizePickerRange(value: unknown): [string, string] | null {
  if (!Array.isArray(value) || value.length < 2) return null;
  const start = parsePickerDate(value[0]);
  const end = parsePickerDate(value[1]);
  if (!start || !end) return null;
  return [formatPickerValue(start), formatPickerValue(end)];
}

function resolveRange(): { startDate: number; endDate: number } {
  if (range.value === "custom" && dateRange.value) {
    const start = parsePickerDate(dateRange.value[0]);
    const end = parsePickerDate(dateRange.value[1]);
    if (start && end) {
      return { startDate: seconds(start), endDate: seconds(end) };
    }
  }
  const { start, end } = presetDates(range.value);
  return { startDate: seconds(start), endDate: seconds(end) };
}

async function loadUsage(
  refresh = false,
  options: { silent?: boolean; notify?: boolean } = {},
): Promise<void> {
  const silent = Boolean(options.silent);
  const notify = options.notify ?? refresh;
  const serial = ++loadSerial;
  if (!silent) loading.value = true;
  try {
    const nextDashboard = await getCodexUsageDashboard({
      ...resolveRange(),
      page: page.value,
      pageSize,
      refresh,
    });
    const nextActivity = await getCodexUsageActivity({ refresh: false }).catch((error) => {
      if (!silent) Message.warning(`加载年度活动失败：${errorText(error)}`);
      return null;
    });
    if (serial !== loadSerial) return;
    dashboard.value = nextDashboard;
    if (nextActivity) activity.value = nextActivity;
    lastLoadedAt.value = Date.now();
    if (!pricingLoaded.value && nextDashboard.pricingConfigs?.length) {
      pricingConfigs.value = nextDashboard.pricingConfigs;
      originalPricingConfigs.value = nextDashboard.pricingConfigs.map((item) => ({ ...item }));
    }
    if (refresh && notify && !silent) Message.success("消耗数据已刷新");
  } catch (error) {
    if (!silent) Message.error(`加载消耗数据失败：${errorText(error)}`);
  } finally {
    if (serial === loadSerial && !silent) loading.value = false;
  }
}

async function loadPricing(): Promise<void> {
  pricingLoading.value = true;
  try {
    const [pricing, config] = await Promise.all([listModelPricing(), getPricingConfig()]);
    pricingList.value = pricing;
    pricingConfigs.value = config;
    originalPricingConfigs.value = config.map((item) => ({ ...item }));
    pricingLoaded.value = true;
  } catch (error) {
    Message.error(`加载费用规则失败：${errorText(error)}`);
  } finally {
    pricingLoading.value = false;
  }
}

function togglePricing(): void {
  pricingCollapsed.value = !pricingCollapsed.value;
  if (!pricingCollapsed.value && !pricingLoaded.value) {
    void loadPricing();
  }
}

async function savePricingConfig(): Promise<void> {
  if (!pricingConfigs.value.every((item) => isValidCost(item.multiplier))) {
    Message.warning("默认倍率必须是非负数字");
    return;
  }
  savingPricingConfig.value = true;
  try {
    const saved = await updatePricingConfig(pricingConfigs.value.map((item) => ({ ...item })));
    pricingConfigs.value = saved;
    originalPricingConfigs.value = saved.map((item) => ({ ...item }));
    Message.success("费用口径已保存");
    await loadUsage(false);
  } catch (error) {
    Message.error(`保存费用口径失败：${errorText(error)}`);
  } finally {
    savingPricingConfig.value = false;
  }
}

function changeRange(next: string | number | boolean): void {
  if (!rangeOptions.some((item) => item.value === next)) return;
  range.value = next as UsageRange;
  page.value = 1;
  syncPickerToPreset(range.value);
  void loadUsage(true, { notify: false });
}

function changeCustomRange(value: unknown): void {
  const normalized = normalizePickerRange(value);
  if (!normalized) {
    range.value = "today";
    page.value = 1;
    syncPickerToPreset("today");
    void loadUsage(true, { notify: false });
    return;
  }
  range.value = "custom";
  dateRange.value = normalized;
  page.value = 1;
  void loadUsage(true, { notify: false });
}

function refreshUsage(): void {
  page.value = 1;
  void loadUsage(true);
}

function changeAutoRefreshInterval(value: unknown): void {
  const next = Number(value);
  if (!autoRefreshOptions.some((item) => item.value === next)) return;
  autoRefreshIntervalMs.value = next;
  try {
    window.localStorage.setItem(autoRefreshStorageKey, String(next));
  } catch {
    // localStorage 不可用时只在当前页面生命周期内生效。
  }
  scheduleAutoRefresh();
}

function changePage(next: number): void {
  page.value = Math.min(Math.max(1, next), totalPages.value);
  void loadUsage(false);
}

function shouldAutoRefresh(): boolean {
  return props.active && range.value === "today" && !loading.value && !pricingModalVisible.value;
}

function refreshPresetRangeForNow(): void {
  if (range.value === "today") syncPickerToPreset("today");
}

function scheduleAutoRefresh(): void {
  if (autoRefreshTimer) window.clearInterval(autoRefreshTimer);
  autoRefreshTimer = undefined;
  if (autoRefreshIntervalMs.value <= 0) return;
  autoRefreshTimer = window.setInterval(() => {
    if (!shouldAutoRefresh()) return;
    refreshPresetRangeForNow();
    void loadUsage(false, { silent: true });
  }, autoRefreshIntervalMs.value);
}

function scheduleActivationRefresh(delay = 280): void {
  if (activationRefreshTimer) window.clearTimeout(activationRefreshTimer);
  activationRefreshTimer = window.setTimeout(() => {
    activationRefreshTimer = undefined;
    window.requestAnimationFrame(() => refreshWhenVisible(true));
  }, delay);
}

function refreshWhenVisible(force = false): void {
  if (loading.value) return;
  if (!props.active || document.visibilityState === "hidden") return;
  const stale = Date.now() - lastLoadedAt.value > 5_000;
  if (!force && !stale) return;
  refreshPresetRangeForNow();
  void loadUsage(false, { silent: true });
}

function handleWindowFocus(): void {
  refreshWhenVisible();
}

function handleVisibilityChange(): void {
  refreshWhenVisible();
}

function resetPricingForm(): void {
  editingModelId.value = null;
  pricingForm.modelId = "";
  pricingForm.displayName = "";
  pricingForm.inputCostPerMillion = "0";
  pricingForm.outputCostPerMillion = "0";
  pricingForm.cacheReadCostPerMillion = "0";
  pricingForm.cacheCreationCostPerMillion = "0";
}

function openAddPricing(): void {
  resetPricingForm();
  pricingModalVisible.value = true;
}

function openEditPricing(item: CodexUsagePricing): void {
  editingModelId.value = item.modelId;
  Object.assign(pricingForm, { ...item });
  pricingModalVisible.value = true;
}

function isValidCost(value: string): boolean {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0;
}

async function savePricing(): Promise<void> {
  if (!pricingForm.modelId.trim() || !pricingForm.displayName.trim()) {
    Message.warning("模型 ID 和显示名称不能为空");
    return;
  }
  const costs = [
    pricingForm.inputCostPerMillion,
    pricingForm.outputCostPerMillion,
    pricingForm.cacheReadCostPerMillion,
    pricingForm.cacheCreationCostPerMillion,
  ];
  if (!costs.every(isValidCost)) {
    Message.warning("单价必须是非负数字");
    return;
  }
  savingPricing.value = true;
  try {
    await updateModelPricing({ ...pricingForm });
    pricingModalVisible.value = false;
    Message.success(editingModelId.value ? "模型单价已更新" : "模型单价已添加");
    await loadPricing();
    await loadUsage(false);
  } catch (error) {
    Message.error(`保存模型单价失败：${errorText(error)}`);
  } finally {
    savingPricing.value = false;
  }
}

async function removePricing(modelId: string): Promise<void> {
  pricingLoading.value = true;
  try {
    await deleteModelPricing(modelId);
    Message.success("模型单价已删除");
    await loadPricing();
    await loadUsage(false);
  } catch (error) {
    Message.error(`删除模型单价失败：${errorText(error)}`);
  } finally {
    pricingLoading.value = false;
  }
}

async function restoreDefaultPricing(): Promise<void> {
  pricingLoading.value = true;
  try {
    pricingList.value = await resetModelPricing();
    Message.success("内置单价已恢复");
    await loadUsage(false);
  } catch (error) {
    Message.error(`恢复内置单价失败：${errorText(error)}`);
  } finally {
    pricingLoading.value = false;
  }
}

function formatTokens(value?: number): string {
  return formatCompactTokens(value);
}

function formatFullNumber(value?: number): string {
  return formatLocalizedNumber(Number(value || 0));
}

function formatUsd(value?: string): string {
  const parsed = Number(value || 0);
  return `$${parsed.toFixed(parsed >= 10 ? 2 : 4)}`;
}

function formatPercent(value?: number): string {
  return `${((value || 0) * 100).toFixed(1)}%`;
}

function activityHourToCellSource(hour: CodexUsageActivityHour): ActivityCell {
  return {
    key: `hour-${hour.hour}`,
    label: hour.label,
    timestamp: hour.timestamp,
    tokens: hour.tokens,
    requests: hour.requests,
    value: 0,
    level: 0,
  };
}

function activityDayToCellSource(day: CodexUsageActivityDay): ActivityCell {
  return {
    key: day.date,
    label: day.date,
    timestamp: day.timestamp,
    tokens: day.tokens,
    requests: day.requests,
    value: 0,
    level: 0,
  };
}

function activityWeekdayLabel(day: CodexUsageActivityDay): string {
  const date = new Date(day.timestamp * 1000);
  const weekday = new Intl.DateTimeFormat(currentLocale(), { weekday: "short" }).format(date);
  return `${date.getMonth() + 1}/${date.getDate()} ${weekday}`;
}

function activityLevel(value: number, max: number): number {
  if (value <= 0 || max <= 0) return 0;
  const ratio = value / max;
  if (ratio >= 0.75) return 4;
  if (ratio >= 0.45) return 3;
  if (ratio >= 0.18) return 2;
  return 1;
}

function activityCellTitle(cell: ActivityCell): string {
  const prefix =
    activityMode.value === "weekly" ? t("当日") : activityMode.value === "cumulative" ? t("累计") : t("时段");
  return `${cell.label}: ${prefix} ${formatFullNumber(cell.value)} Tokens, ${formatLocalizedCount(cell.requests, "次")}`;
}

function formatTime(timestamp: number): string {
  if (!timestamp) return "--";
  return new Intl.DateTimeFormat(currentLocale(), {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(timestamp * 1000));
}

function compactAxisNumber(value: number): string {
  if (value >= 1_000_000) return `${Math.round(value / 1_000)}k`;
  if (value >= 1_000) return `${Math.round(value / 1_000)}k`;
  return String(Math.round(value));
}

function tooltipParams(value: unknown): UsageTooltipParam[] {
  if (Array.isArray(value)) return value as UsageTooltipParam[];
  return value ? [value as UsageTooltipParam] : [];
}

function tooltipValue(param: UsageTooltipParam): number {
  if (Array.isArray(param.value)) {
    const value = param.value[param.value.length - 1];
    return typeof value === "number" ? value : Number(value || 0);
  }
  return Number(param.value || 0);
}

function formatChartTooltip(params: unknown): string {
  const items = tooltipParams(params);
  const label = Array.isArray(items[0]?.value) ? items[0]?.value?.[0] : "";
  const rows = items
    .map((item) => {
      const value = tooltipValue(item);
      const formatted =
        item.seriesName === t("预估费用") ? formatUsd(String(value)) : formatFullNumber(value);
      return `<div class="usage-echart-tooltip-row">${item.marker || ""}<span>${item.seriesName}</span><strong>${formatted}</strong></div>`;
    })
    .join("");
  return `<div class="usage-echart-tooltip"><strong>${label || "--"}</strong>${rows}</div>`;
}

function trendSeriesData(key: "inputTokens" | "outputTokens" | "cacheReadTokens" | "cacheCreationTokens" | "totalCost") {
  return trends.value.map((item) => [
    item.label,
    key === "totalCost" ? Number(item.totalCost || 0) : Number(item[key] || 0),
  ]);
}

async function loadEchartsApi(): Promise<EChartsCoreApi> {
  if (echartsApi) return echartsApi;
  if (!echartsLoading) {
    echartsLoading = Promise.all([
      import("echarts/core"),
      import("echarts/charts"),
      import("echarts/components"),
      import("echarts/renderers"),
    ]).then(([core, charts, components, renderers]) => {
      core.use([
        charts.LineChart,
        components.GridComponent,
        components.LegendComponent,
        components.TooltipComponent,
        renderers.CanvasRenderer,
      ]);
      echartsApi = core;
      return core;
    });
  }
  return echartsLoading;
}

function usageChartOption(core: EChartsCoreApi): EChartsCoreOption {
  const labels = trends.value.map((item) => item.label);
  return {
    color: ["#ef4444", "#f97316", "#a855f7", "#3b82f6", "#22c55e"],
    animation: true,
    animationDuration: 900,
    animationEasing: "cubicOut",
    grid: {
      top: 28,
      right: 48,
      bottom: 72,
      left: 54,
      containLabel: true,
    },
    tooltip: {
      trigger: "axis",
      confine: true,
      appendToBody: true,
      backgroundColor: "rgba(255, 255, 255, 0.96)",
      borderColor: "rgba(85, 113, 156, 0.18)",
      borderWidth: 1,
      padding: 0,
      extraCssText: "box-shadow: 0 18px 42px rgba(15, 23, 42, 0.18); border-radius: 8px;",
      axisPointer: {
        type: "line",
        lineStyle: {
          color: "rgba(100, 116, 139, 0.38)",
          width: 1,
        },
      },
      formatter: formatChartTooltip,
    },
    legend: {
      bottom: 10,
      left: "center",
      itemWidth: 18,
      itemHeight: 8,
      icon: "roundRect",
      textStyle: {
        color: "#64748b",
        fontSize: 12,
        fontWeight: 600,
      },
      data: ["预估费用", "缓存写入", "缓存复用", "输入", "输出"].map(t),
    },
    xAxis: {
      type: "category",
      boundaryGap: false,
      data: labels,
      axisTick: { show: false },
      axisLine: { lineStyle: { color: "rgba(85, 113, 156, 0.18)" } },
      axisLabel: {
        interval: isHourlyTrend.value ? 0 : "auto",
        hideOverlap: true,
        margin: 18,
        color: "#8a97aa",
        fontSize: 12,
      },
    },
    yAxis: [
      {
        type: "value",
        min: 0,
        splitNumber: 5,
        axisLabel: {
          color: "#8a97aa",
          formatter: compactAxisNumber,
        },
        splitLine: {
          lineStyle: {
            color: "rgba(85, 113, 156, 0.16)",
            type: "dashed",
          },
        },
      },
      {
        type: "value",
        min: 0,
        axisLabel: {
          color: "#8a97aa",
          formatter: (value: number) => `$${Number(value).toFixed(0)}`,
        },
        splitLine: { show: false },
      },
    ],
    series: [
      {
        name: t("预估费用"),
        type: "line",
        yAxisIndex: 1,
        smooth: 0.42,
        showSymbol: false,
        symbol: "circle",
        symbolSize: 7,
        data: trendSeriesData("totalCost"),
        lineStyle: { width: 2.5, type: "dashed" },
        emphasis: { focus: "series" },
      },
      {
        name: t("缓存写入"),
        type: "line",
        smooth: 0.42,
        showSymbol: false,
        symbol: "circle",
        symbolSize: 7,
        data: trendSeriesData("cacheCreationTokens"),
        lineStyle: { width: 2.5 },
        emphasis: { focus: "series" },
      },
      {
        name: t("缓存复用"),
        type: "line",
        smooth: 0.42,
        showSymbol: false,
        symbol: "circle",
        symbolSize: 8,
        data: trendSeriesData("cacheReadTokens"),
        lineStyle: { width: 3.5 },
        areaStyle: {
          color: new core.graphic.LinearGradient(0, 0, 0, 1, [
            { offset: 0, color: "rgba(168, 85, 247, 0.24)" },
            { offset: 1, color: "rgba(168, 85, 247, 0)" },
          ]),
        },
        emphasis: { focus: "series" },
      },
      {
        name: t("输入"),
        type: "line",
        smooth: 0.42,
        showSymbol: false,
        symbol: "circle",
        symbolSize: 7,
        data: trendSeriesData("inputTokens"),
        lineStyle: { width: 3 },
        emphasis: { focus: "series" },
      },
      {
        name: t("输出"),
        type: "line",
        smooth: 0.42,
        showSymbol: false,
        symbol: "circle",
        symbolSize: 7,
        data: trendSeriesData("outputTokens"),
        lineStyle: { width: 3 },
        emphasis: { focus: "series" },
      },
    ],
  };
}

async function ensureUsageChart(): Promise<ECharts | null> {
  if (!chartContainer.value) return null;
  const core = await loadEchartsApi();
  if (!usageChart.value) {
    usageChart.value = core.init(chartContainer.value, undefined, { renderer: "canvas" });
    chartResizeObserver = new ResizeObserver(() => usageChart.value?.resize());
    chartResizeObserver.observe(chartContainer.value);
  }
  return usageChart.value;
}

function renderUsageChart(): void {
  void nextTick(async () => {
    if (!trends.value.length) {
      chartResizeObserver?.disconnect();
      chartResizeObserver = null;
      usageChart.value?.dispose();
      usageChart.value = null;
      return;
    }
    const core = await loadEchartsApi();
    if (!trends.value.length) return;
    const chart = await ensureUsageChart();
    chart?.setOption(usageChartOption(core), true);
  });
}

onMounted(() => {
  syncPickerToPreset("today");
  window.setTimeout(() => {
    void loadUsage(false);
    renderUsageChart();
  }, props.active ? 120 : 0);
  scheduleAutoRefresh();
  window.addEventListener("focus", handleWindowFocus);
  document.addEventListener("visibilitychange", handleVisibilityChange);
});

watch([trends, isHourlyTrend, currentLanguage], renderUsageChart, { deep: true });

watch(
  () => props.active,
  (active) => {
    if (active) {
      scheduleActivationRefresh();
    } else if (activationRefreshTimer) {
      window.clearTimeout(activationRefreshTimer);
      activationRefreshTimer = undefined;
    }
  },
);

onBeforeUnmount(() => {
  if (autoRefreshTimer) window.clearInterval(autoRefreshTimer);
  if (activationRefreshTimer) window.clearTimeout(activationRefreshTimer);
  window.removeEventListener("focus", handleWindowFocus);
  document.removeEventListener("visibilitychange", handleVisibilityChange);
  chartResizeObserver?.disconnect();
  chartResizeObserver = null;
  usageChart.value?.dispose();
  usageChart.value = null;
});
</script>

<template>
  <section class="usage-panel">
    <div class="usage-head">
      <div>
        <h2>{{ t("消耗看板") }}</h2>
        <p>{{ t("从本机会话记录汇总 Tokens、缓存复用和预估费用") }}</p>
      </div>
      <div class="usage-controls">
        <a-radio-group :model-value="range" type="button" @change="changeRange">
          <a-radio v-for="item in rangeOptions" :key="item.value" :value="item.value">
            {{ t(item.label) }}
          </a-radio>
        </a-radio-group>
        <a-range-picker
          :model-value="dateRange"
          show-time
          format="YYYY/MM/DD HH:mm:ss"
          class="usage-range-picker"
          @change="changeCustomRange"
        />
        <a-select
          :model-value="autoRefreshIntervalMs"
          class="usage-auto-refresh-select"
          @change="changeAutoRefreshInterval"
        >
          <template #prefix><icon-refresh /></template>
          <a-option v-for="item in autoRefreshOptions" :key="item.value" :value="item.value">
            {{ t(item.label) }}
          </a-option>
        </a-select>
        <a-button :loading="loading" @click="refreshUsage">
          <template #icon><icon-refresh /></template>
          {{ t("刷新") }}
        </a-button>
      </div>
    </div>

    <a-spin :loading="loading" dot>
      <div class="usage-dashboard-grid">
        <div class="usage-main-column">
          <div class="usage-hero">
            <div class="usage-hero-main">
              <div class="usage-app-mark"><icon-robot /></div>
              <div>
                <span>{{ t("本地 Codex 消耗") }}</span>
                <strong>{{ formatFullNumber(summary?.realTotalTokens) }}</strong>
                <small>≈ {{ formatTokens(summary?.realTotalTokens) }}</small>
              </div>
            </div>
            <div class="usage-hero-side">
              <div>
                <span>{{ t("总请求数") }}</span>
                <strong>{{ formatFullNumber(summary?.totalRequests) }}</strong>
              </div>
              <div>
                <span>{{ t("预估费用") }}</span>
                <strong>{{ formatUsd(summary?.totalCost) }}</strong>
              </div>
            </div>
          </div>

          <div class="usage-metrics">
            <article>
              <span><icon-arrow-down /> {{ t("输入 Tokens") }}</span>
              <strong>{{ formatTokens(summary?.totalInputTokens) }}</strong>
            </article>
            <article>
              <span><icon-arrow-up /> {{ t("输出 Tokens") }}</span>
              <strong>{{ formatTokens(summary?.totalOutputTokens) }}</strong>
            </article>
            <article>
              <span><icon-storage /> {{ t("缓存写入") }}</span>
              <strong>{{ formatTokens(summary?.totalCacheCreationTokens) }}</strong>
            </article>
            <article>
              <span><icon-thunderbolt /> {{ t("缓存复用") }}</span>
              <strong>{{ formatTokens(summary?.totalCacheReadTokens) }}</strong>
            </article>
            <article>
              <span>{{ t("复用占比") }}</span>
              <strong>{{ formatPercent(summary?.cacheHitRate) }}</strong>
              <div class="usage-hit-bar">
                <i :style="{ width: formatPercent(summary?.cacheHitRate) }" />
              </div>
            </article>
          </div>

          <div class="usage-chart-card">
            <div class="usage-card-title">
              <strong>{{ t("时段消耗曲线") }}</strong>
            </div>
            <div v-if="trends.length" class="usage-chart">
              <div ref="chartContainer" class="usage-echart" />
            </div>
            <a-empty v-else :description="t('暂无趋势数据')" />
          </div>

          <div class="usage-activity-card">
            <div class="usage-activity-summary">
              <article>
                <strong>{{ formatTokens(activitySummary?.totalTokens) }}</strong>
                <span>{{ t("累计 Token 数") }}</span>
              </article>
              <article>
                <strong>{{ formatTokens(activitySummary?.peakDayTokens) }}</strong>
                <span>{{ t("峰值 Token 数") }}</span>
              </article>
              <article>
                <strong>{{ formatLocalizedCount(activitySummary?.currentStreakDays ?? 0, "天") }}</strong>
                <span>{{ t("当前连续天数") }}</span>
              </article>
              <article>
                <strong>{{ formatLocalizedCount(activitySummary?.longestStreakDays ?? 0, "天") }}</strong>
                <span>{{ t("最长连续天数") }}</span>
              </article>
            </div>

            <div class="usage-card-title usage-activity-title">
              <strong>{{ t("Token 活动") }}</strong>
              <div class="usage-activity-mode">
                <button
                  v-for="item in activityModeOptions"
                  :key="item.value"
                  :class="{ active: activityMode === item.value }"
                  type="button"
                  @click="activityMode = item.value"
                >
                  {{ t(item.label) }}
                </button>
              </div>
            </div>
            <div v-if="activityMode === 'daily'" class="usage-activity-bars daily">
              <article
                v-for="block in dailyActivityBlocks"
                :key="block.key"
                class="usage-activity-block"
                :class="`level-${block.level}`"
                :title="activityCellTitle(block)"
              >
                <div class="usage-activity-block-head">
                  <strong>{{ block.label }}</strong>
                  <span>{{ block.subtitle }}</span>
                </div>
                <div class="usage-activity-track">
                  <i :style="{ width: `${block.percent}%` }" />
                </div>
                <div class="usage-activity-block-foot">
                  <span>{{ formatTokens(block.value) }}</span>
                  <em>{{ formatLocalizedCount(block.requests, "次") }}</em>
                </div>
              </article>
            </div>
            <div v-else-if="activityMode === 'weekly'" class="usage-activity-bars weekly">
              <article
                v-for="block in weeklyActivityBlocks"
                :key="block.key"
                class="usage-activity-block"
                :class="`level-${block.level}`"
                :title="activityCellTitle(block)"
              >
                <div class="usage-activity-block-head">
                  <strong>{{ block.label }}</strong>
                  <span>{{ block.subtitle }}</span>
                </div>
                <div class="usage-activity-track">
                  <i :style="{ width: `${block.percent}%` }" />
                </div>
                <div class="usage-activity-block-foot">
                  <span>{{ formatTokens(block.value) }}</span>
                  <em>{{ formatLocalizedCount(block.requests, "次") }}</em>
                </div>
              </article>
            </div>
            <div v-else-if="activityCells.length" class="usage-activity-map">
              <div
                class="usage-activity-grid"
                :style="{ '--activity-columns': activityColumnCount, '--activity-rows': activityRowCount }"
              >
                <i
                  v-for="cell in activityCells"
                  :key="cell.key"
                  class="usage-activity-cell"
                  :class="`level-${cell.level}`"
                  :title="activityCellTitle(cell)"
                />
              </div>
              <div
                class="usage-activity-months"
                :style="{ '--activity-columns': activityColumnCount }"
              >
                <span
                  v-for="item in activityLabels"
                  :key="item.key"
                  :style="{ gridColumn: item.column }"
                >
                  {{ item.label }}
                </span>
              </div>
            </div>
            <a-empty v-else :description="t('暂无活动数据')" />
          </div>

          <div class="usage-table-card">
            <div class="usage-tabs">
              <button :class="{ active: activeUsageTab === 'logs' }" type="button" @click="activeUsageTab = 'logs'">
                <icon-list /> {{ t("调用流水") }}
              </button>
              <button :class="{ active: activeUsageTab === 'providers' }" type="button" @click="activeUsageTab = 'providers'">
                <icon-thunderbolt /> {{ t("来源汇总") }}
              </button>
              <button :class="{ active: activeUsageTab === 'models' }" type="button" @click="activeUsageTab = 'models'">
                <icon-bar-chart /> {{ t("模型用量") }}
              </button>
            </div>

            <div v-if="activeUsageTab === 'logs'">
              <div class="usage-card-title">
                <strong>{{ t("调用记录") }}</strong>
                <span>{{ formatLocalizedCount(totalLogs, "条记录") }}</span>
              </div>
              <div class="usage-log-table">
                <table>
                  <thead>
                    <tr>
                      <th>{{ t("时间") }}</th>
                      <th>{{ t("来源") }}</th>
                      <th>{{ t("计费模型") }}</th>
                      <th>{{ t("输入") }}</th>
                      <th>{{ t("输出") }}</th>
                      <th>{{ t("费用") }}</th>
                      <th>{{ t("状态") }}</th>
                      <th>{{ t("来源") }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="log in logs" :key="log.requestId">
                      <td>{{ formatTime(log.createdAt) }}</td>
                      <td>{{ log.providerName }}</td>
                      <td><code>{{ log.model }}</code></td>
                      <td>
                        {{ formatFullNumber(log.inputTokens) }}
                        <small v-if="log.cacheReadTokens">R{{ formatFullNumber(log.cacheReadTokens) }}</small>
                      </td>
                      <td>{{ formatFullNumber(log.outputTokens) }}</td>
                      <td>{{ formatUsd(log.totalCost) }}</td>
                      <td><span class="usage-status">{{ log.statusCode }}</span></td>
                      <td><code>{{ log.dataSource }}</code></td>
                    </tr>
                    <tr v-if="!logs.length">
                      <td colspan="8">
                        <a-empty :description="t('暂无使用记录')" />
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <div class="usage-pagination">
                <span>{{ t(`第 ${page} / ${totalPages} 页`) }}</span>
                <a-button size="small" :disabled="page <= 1" @click="changePage(page - 1)">
                  <template #icon><icon-left /></template>
                </a-button>
                <a-button size="small" :disabled="page >= totalPages" @click="changePage(page + 1)">
                  <template #icon><icon-right /></template>
                </a-button>
              </div>
            </div>

            <div v-else-if="activeUsageTab === 'providers'" class="usage-stat-table">
              <table>
                <thead>
                  <tr>
                    <th>{{ t("来源") }}</th>
                    <th>{{ t("请求数") }}</th>
                    <th>Tokens</th>
                    <th>{{ t("费用") }}</th>
                    <th>{{ t("成功率") }}</th>
                    <th>{{ t("平均延迟") }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="item in providerStats" :key="item.providerId">
                    <td>{{ item.providerName }}</td>
                    <td>{{ formatFullNumber(item.requestCount) }}</td>
                    <td>{{ formatFullNumber(item.totalTokens) }}</td>
                    <td>{{ formatUsd(item.totalCost) }}</td>
                    <td>{{ item.successRate.toFixed(1) }}%</td>
                    <td>{{ item.avgLatencyMs }}ms</td>
                  </tr>
                  <tr v-if="!providerStats.length">
                    <td colspan="6"><a-empty :description="t('暂无来源数据')" /></td>
                  </tr>
                </tbody>
              </table>
            </div>

            <div v-else class="usage-stat-table">
              <table>
                <thead>
                  <tr>
                    <th>{{ t("模型") }}</th>
                    <th>{{ t("请求数") }}</th>
                    <th>Tokens</th>
                    <th>{{ t("费用") }}</th>
                    <th>{{ t("单次均价") }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="item in modelStats" :key="item.model">
                    <td><code>{{ item.model }}</code></td>
                    <td>{{ formatFullNumber(item.requestCount) }}</td>
                    <td>{{ formatFullNumber(item.totalTokens) }}</td>
                    <td>{{ formatUsd(item.totalCost) }}</td>
                    <td>{{ formatUsd(item.avgCostPerRequest) }}</td>
                  </tr>
                  <tr v-if="!modelStats.length">
                    <td colspan="5"><a-empty :description="t('暂无模型用量')" /></td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>

          <div class="usage-pricing-card">
        <button class="usage-pricing-head" type="button" @click="togglePricing">
          <span>
            <icon-settings />
            <strong>{{ t("费用规则") }}</strong>
            <small>{{ t("维护 Codex 统计使用的模型单价和倍率") }}</small>
          </span>
          <icon-down :class="{ rotated: !pricingCollapsed }" />
        </button>

        <div v-show="!pricingCollapsed" class="usage-pricing-body">
          <div class="usage-pricing-section">
            <div class="usage-card-title">
              <div>
                <strong>{{ t("Codex 计费口径") }}</strong>
                <span>{{ t("设置统计倍率与模型识别来源") }}</span>
              </div>
              <a-button
                type="primary"
                :disabled="!pricingConfigDirty"
                :loading="savingPricingConfig"
                @click="savePricingConfig"
              >
                {{ t("保存") }}
              </a-button>
            </div>
            <div class="usage-pricing-defaults">
              <table>
                <thead>
                  <tr>
                    <th>{{ t("应用") }}</th>
                    <th>{{ t("默认倍率") }}</th>
                    <th>{{ t("计费模式") }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="item in pricingConfigs" :key="item.app">
                    <td>{{ item.app }}</td>
                    <td>
                      <a-input
                        v-model="item.multiplier"
                        inputmode="decimal"
                        class="usage-config-input"
                        placeholder="1"
                      />
                    </td>
                    <td>
                      <a-select v-model="item.pricingModelSource" class="usage-config-select">
                        <a-option value="response">{{ t("返回模型") }}</a-option>
                        <a-option value="request">{{ t("请求模型") }}</a-option>
                      </a-select>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>

          <div class="usage-pricing-section">
            <div class="usage-card-title">
              <div>
                <strong>{{ t("模型单价（每百万 Tokens）") }}</strong>
                <span>{{ formatLocalizedCount(pricingList.length, "条规则") }}</span>
              </div>
              <div class="usage-pricing-actions">
                <a-popconfirm :content="t('恢复内置 GPT/Codex 单价会覆盖当前 pricing.json，确定继续？')" @ok="restoreDefaultPricing">
                  <a-button :loading="pricingLoading">
                    <template #icon><icon-refresh /></template>
                    {{ t("恢复默认") }}
                  </a-button>
                </a-popconfirm>
                <a-button type="primary" @click="openAddPricing">
                  <template #icon><icon-plus /></template>
                  {{ t("添加") }}
                </a-button>
              </div>
            </div>

            <a-spin :loading="pricingLoading" dot>
              <div class="usage-pricing-table">
                <table>
                  <thead>
                    <tr>
                      <th>{{ t("模型") }}</th>
                      <th>{{ t("显示名称") }}</th>
                      <th>{{ t("输入单价") }}</th>
                      <th>{{ t("输出单价") }}</th>
                      <th>{{ t("缓存复用") }}</th>
                      <th>{{ t("缓存写入") }}</th>
                      <th>{{ t("操作") }}</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="item in pricingList" :key="item.modelId">
                      <td><code>{{ item.modelId }}</code></td>
                      <td>{{ item.displayName }}</td>
                      <td>${{ item.inputCostPerMillion }}</td>
                      <td>${{ item.outputCostPerMillion }}</td>
                      <td>${{ item.cacheReadCostPerMillion }}</td>
                      <td>${{ item.cacheCreationCostPerMillion }}</td>
                      <td>
                        <a-space>
                          <a-button size="mini" @click="openEditPricing(item)">
                            <template #icon><icon-edit /></template>
                          </a-button>
                          <a-popconfirm :content="t('确定删除这条模型单价？')" @ok="removePricing(item.modelId)">
                            <a-button size="mini" status="danger">
                              <template #icon><icon-delete /></template>
                            </a-button>
                          </a-popconfirm>
                        </a-space>
                      </td>
                    </tr>
                    <tr v-if="!pricingList.length">
                      <td colspan="7">
                        <a-empty :description="t('暂无模型单价')" />
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </a-spin>
          </div>
        </div>
          </div>
        </div>

        <aside class="usage-side-column">
          <section class="usage-side-card">
            <div class="usage-card-title">
              <strong>{{ t("本期概览") }}</strong>
              <span>Codex</span>
            </div>
            <article class="usage-side-metric">
              <span>{{ t("主要来源") }}</span>
              <strong>{{ topProvider?.providerName || "--" }}</strong>
              <small>{{ formatUsd(topProvider?.totalCost) }} · {{ formatTokens(topProvider?.totalTokens) }}</small>
            </article>
            <article class="usage-side-metric">
              <span>{{ t("主要模型") }}</span>
              <strong>{{ topModel?.model || "--" }}</strong>
              <small>{{ formatUsd(topModel?.totalCost) }} · {{ formatTokens(topModel?.totalTokens) }}</small>
            </article>
            <article class="usage-side-metric">
              <span>{{ t("费用倍率") }}</span>
              <strong>{{ codexPricingConfig.multiplier }}x</strong>
              <small>{{ codexPricingConfig.pricingModelSource === "request" ? t("请求模型") : t("返回模型") }}</small>
            </article>
          </section>

          <section class="usage-side-card">
            <div class="usage-card-title">
              <strong>{{ t("来源分布") }}</strong>
              <span>{{ formatLocalizedCount(providerStats.length, "个") }}</span>
            </div>
            <article v-for="item in providerStats.slice(0, 5)" :key="item.providerId" class="usage-rank-row">
              <div>
                <strong>{{ item.providerName }}</strong>
                <span>{{ formatLocalizedCount(item.requestCount, "次") }} · {{ formatUsd(item.totalCost) }}</span>
              </div>
              <b>{{ formatTokens(item.totalTokens) }}</b>
            </article>
            <a-empty v-if="!providerStats.length" :description="t('暂无来源数据')" />
          </section>

          <section class="usage-side-card">
            <div class="usage-card-title">
              <strong>{{ t("模型分布") }}</strong>
              <span>{{ formatLocalizedCount(modelStats.length, "个") }}</span>
            </div>
            <article v-for="item in modelStats.slice(0, 6)" :key="item.model" class="usage-rank-row">
              <div>
                <strong>{{ item.model }}</strong>
                <span>{{ formatLocalizedCount(item.requestCount, "次") }} · {{ t("平均") }} {{ formatUsd(item.avgCostPerRequest) }}</span>
              </div>
              <b>{{ formatTokens(item.totalTokens) }}</b>
            </article>
            <a-empty v-if="!modelStats.length" :description="t('暂无模型用量')" />
          </section>
        </aside>
      </div>

      <a-alert
        v-if="dashboard?.errors.length"
        type="warning"
        :content="t(`有 ${dashboard.errors.length} 个会话文件暂时无法读取，已跳过这些文件。`)"
      />
    </a-spin>

    <a-modal
      v-model:visible="pricingModalVisible"
      :title="editingModelId ? t('编辑模型单价') : t('添加模型单价')"
      :footer="false"
      width="620px"
    >
      <a-form :model="pricingForm" layout="vertical">
        <a-form-item :label="t('模型 ID')">
          <a-input v-model="pricingForm.modelId" :placeholder="t('例如 gpt-5-codex')" />
        </a-form-item>
        <a-form-item :label="t('显示名称')">
          <a-input v-model="pricingForm.displayName" :placeholder="t('例如 GPT-5 Codex')" />
        </a-form-item>
        <div class="usage-pricing-form-grid">
          <a-form-item :label="t('输入单价 / 1M')">
            <a-input v-model="pricingForm.inputCostPerMillion" />
          </a-form-item>
          <a-form-item :label="t('输出单价 / 1M')">
            <a-input v-model="pricingForm.outputCostPerMillion" />
          </a-form-item>
          <a-form-item :label="t('缓存复用 / 1M')">
            <a-input v-model="pricingForm.cacheReadCostPerMillion" />
          </a-form-item>
          <a-form-item :label="t('缓存写入 / 1M')">
            <a-input v-model="pricingForm.cacheCreationCostPerMillion" />
          </a-form-item>
        </div>
      </a-form>
      <div class="usage-modal-actions">
        <a-button @click="pricingModalVisible = false">{{ t("取消") }}</a-button>
        <a-button type="primary" :loading="savingPricing" @click="savePricing">{{ t("保存") }}</a-button>
      </div>
    </a-modal>
  </section>
</template>
