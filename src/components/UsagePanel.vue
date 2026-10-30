<script setup lang="ts">
import UsagePricingSync from "./UsagePricingSync.vue";
import OpenAiIcon from "./OpenAiIcon.vue";
import { modelsDevPricingRevision } from "../services/modelsDev";
import { calendarDateKey, calendarParts, effectiveTimeZone, formatAppDateTime, formatWallTime, nextCalendarDayStart, parseWallTime, usageCalendarRange } from "../timeZone";
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
  formatLocalizedCount,
  formatLocalizedNumber,
  t,
} from "../i18n";
import { formatUsageSpeed, usageRequestSpeed, formatUnitPrice, formatUsageTokens, normalizeTokenDisplayUnit, tokenDisplayStorageKey } from "../usageDisplay";
import {
  DEFAULT_CODEX_INSTANCE_ID,
  instanceDisplayName,
  type CodexInstance,
} from "../services/instances";

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
  instances?: CodexInstance[];
  currentInstanceId?: string;
}>(), {
  active: true,
  instances: () => [],
  currentInstanceId: DEFAULT_CODEX_INSTANCE_ID,
});

const pageSize = ref(20);
const tokenDisplayUnit = ref(readTokenDisplayUnit());
const loading = ref(false);
const pricingLoading = ref(false);
const savingPricing = ref(false);
const savingPricingConfig = ref(false);
const pricingLoaded = ref(false);
const pricingModalVisible = ref(false);
const activeUsageTab = ref("logs");
const pricingSearch = ref("");
const range = ref<UsageRange>("today");
const page = ref(1);
const chartContainer = ref<HTMLDivElement | null>(null);
const usageChart = shallowRef<ECharts | null>(null);
const dateRange = ref<[string, string] | undefined>();
const dashboard = ref<CodexUsageDashboard | null>(null);
const activity = ref<CodexUsageActivity | null>(null);
const activityMode = ref<ActivityMode>("daily");
const pricingList = ref<CodexUsagePricing[]>([]);
const filteredPricing = computed(() => {
  const query = pricingSearch.value.trim().toLowerCase();
  return pricingList.value.filter((item) => !query || `${item.modelId} ${item.displayName}`.toLowerCase().includes(query));
});
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
const usageInstanceStorageKey = "codex-switcher:usage-instance-scope";
const allInstancesScope = "__all__";
const autoRefreshOptions: AutoRefreshOption[] = [
  { value: 0, label: "关闭" },
  { value: 5_000, label: "5s" },
  { value: 10_000, label: "10s" },
  { value: 30_000, label: "30s" },
  { value: 60_000, label: "60s" },
];
const autoRefreshIntervalMs = ref(readAutoRefreshInterval());
const selectedUsageInstanceId = ref(readUsageInstanceScope(props.currentInstanceId));

const hasManagedInstances = computed(() => props.instances.some((instance) => !instance.isDefault));

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
        label: formatAppDateTime(day.timestamp * 1000, { month: "short" }),
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
      label: t(segment.label),
      subtitle: segment.subtitle,
      detail: `${formatFullNumber(tokens)} Tokens · ${formatLocalizedCount(requests, "次")}`,
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
      detail: `${formatFullNumber(value)} Tokens · ${formatLocalizedCount(day.requests, "次")}`,
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
let presetBoundaryTimer: number | undefined;
let loadSerial = 0;
let silentLoadInFlight = false;
let presetBoundaryKey = localDayKey(new Date());
let presetRefreshPending = false;
let disposed = false;

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

function readUsageInstanceScope(fallbackInstanceId: string): string {
  try {
    return window.localStorage.getItem(usageInstanceStorageKey)?.trim()
      || fallbackInstanceId.trim()
      || DEFAULT_CODEX_INSTANCE_ID;
  } catch {
    return fallbackInstanceId.trim() || DEFAULT_CODEX_INSTANCE_ID;
  }
}

function persistUsageInstanceScope(value: string): void {
  try {
    window.localStorage.setItem(usageInstanceStorageKey, value);
  } catch {
    // localStorage 不可用时只在当前页面生命周期内生效。
  }
}

function usageInstanceQuery(): { instanceId: string | null; allInstances: boolean } {
  if (selectedUsageInstanceId.value === allInstancesScope) {
    return { instanceId: null, allInstances: true };
  }
  return {
    instanceId: selectedUsageInstanceId.value || DEFAULT_CODEX_INSTANCE_ID,
    allInstances: false,
  };
}

function errorText(error: unknown): string {
  return String(error instanceof Error ? error.message : error).replace(/^Error:\s*/, "");
}

function formatPickerValue(date: Date): string {
  return formatWallTime(date, true);
}

function localDayKey(date: Date): string {
  return calendarDateKey(date);
}

function seconds(date: Date): number {
  return Math.floor(date.getTime() / 1000);
}

function presetDates(preset: UsageRange): { start: Date; end: Date } {
  return usageCalendarRange(preset);
}

function syncPickerToPreset(preset: UsageRange): void {
  const { start, end } = presetDates(preset);
  dateRange.value = [formatPickerValue(start), formatPickerValue(end)];
}

function parsePickerDate(value: unknown): Date | null {
  if (value instanceof Date) {
    return Number.isNaN(value.getTime()) ? null : new Date(value.getTime());
  }
  if (typeof value === "number") {
    const parsed = new Date(value > 10_000_000_000 ? value : value * 1000);
    return Number.isNaN(parsed.getTime()) ? null : parsed;
  }
  if (typeof value === "string" && value.trim()) {
    const timestamp = parseWallTime(value.trim());
    return timestamp === null ? null : new Date(timestamp);
  }
  return null;
}

function normalizePickerRange(value: unknown): [string, string] | null {
  if (!Array.isArray(value) || value.length < 2) return null;
  const start = parsePickerDate(value[0]);
  const end = parsePickerDate(value[1]);
  if (!start || !end || start.getTime() > end.getTime()) return null;
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
  dateRange.value = [formatPickerValue(start), formatPickerValue(end)];
  return { startDate: seconds(start), endDate: seconds(end) };
}

async function loadUsage(
  refresh = false,
  options: { silent?: boolean; notify?: boolean } = {},
): Promise<void> {
  if (disposed) return;
  const silent = Boolean(options.silent);
  if (silent && silentLoadInFlight) return;
  presetRefreshPending = false;
  if (silent) silentLoadInFlight = true;
  const notify = options.notify ?? refresh;
  const serial = ++loadSerial;
  if (!silent) loading.value = true;
  try {
    const nextDashboard = await getCodexUsageDashboard({
      ...resolveRange(),
      ...usageInstanceQuery(),
      page: page.value,
      pageSize: pageSize.value,
      refresh,
    });
    let activityError: unknown;
    const nextActivity = await getCodexUsageActivity({
      refresh: false,
      ...usageInstanceQuery(),
    }).catch((error) => {
      activityError = error;
      return null;
    });
    if (serial !== loadSerial) return;
    if (activityError && !silent) {
      Message.warning(`加载年度活动失败：${errorText(activityError)}`);
    }
    dashboard.value = nextDashboard;
    if (nextActivity) activity.value = nextActivity;
    lastLoadedAt.value = Date.now();
    if (!pricingLoaded.value && nextDashboard.pricingConfigs?.length) {
      pricingConfigs.value = nextDashboard.pricingConfigs;
      originalPricingConfigs.value = nextDashboard.pricingConfigs.map((item) => ({ ...item }));
    }
    if (nextDashboard.errors.length) {
      // 无法自动归属的会话文件只记录到控制台供排查，不再在页面上打扰用户；
      // 后端刷新时会尽量自动处理（子代理线程、独立计数的 fork 等）。
      console.warn("[usage] 未能自动归属的会话统计项：", nextDashboard.errors);
    }
    if (refresh && notify && !silent) {
      Message.success(t("消耗数据已刷新"));
    }
  } catch (error) {
    if (serial !== loadSerial) return;
    if (!silent) Message.error(`加载消耗数据失败：${errorText(error)}`);
  } finally {
    if (silent) silentLoadInFlight = false;
    if (serial === loadSerial && !silent) loading.value = false;
    if (
      presetRefreshPending
      && !loading.value
      && !silentLoadInFlight
      && !disposed
      && props.active
      && document.visibilityState !== "hidden"
    ) {
      presetRefreshPending = false;
      void loadUsage(false, { silent: true });
    }
  }
}

async function loadPricing(preserveConfigDraft = false): Promise<void> {
  pricingLoading.value = true;
  try {
    const [pricing, config] = await Promise.all([listModelPricing(), getPricingConfig()]);
    pricingList.value = pricing;
    if (!preserveConfigDraft || !pricingConfigDirty.value) {
      pricingConfigs.value = config;
      originalPricingConfigs.value = config.map((item) => ({ ...item }));
    }
    pricingLoaded.value = true;
  } catch (error) {
    Message.error(`加载费用规则失败：${errorText(error)}`);
  } finally {
    pricingLoading.value = false;
  }
}

watch(modelsDevPricingRevision, () => {
  if (pricingLoaded.value) void loadPricing(true);
  void loadUsage(false);
});

watch(activeUsageTab, (tab) => {
  if (tab === "pricing" && !pricingLoaded.value) void loadPricing();
});

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
  presetBoundaryKey = localDayKey(new Date());
  page.value = 1;
  syncPickerToPreset(range.value);
  void loadUsage(false, { notify: false });
}

function changeCustomRange(value: unknown): void {
  const normalized = normalizePickerRange(value);
  if (!normalized) {
    Message.warning("请选择有效的开始和结束时间，日期必须真实存在且开始时间不能晚于结束时间。");
    return;
  }
  range.value = "custom";
  dateRange.value = normalized;
  page.value = 1;
  void loadUsage(false, { notify: false });
}

function refreshUsage(): void {
  page.value = 1;
  void loadUsage(true);
}

function changeUsageInstance(value: unknown): void {
  if (typeof value !== "string" || !value.trim()) return;
  const next = value.trim();
  if (
    next !== allInstancesScope
    && !props.instances.some((instance) => instance.id === next)
  ) {
    return;
  }
  selectedUsageInstanceId.value = next;
  persistUsageInstanceScope(next);
  page.value = 1;
  void loadUsage(false, { notify: false });
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
  page.value = Math.min(Math.max(1, next), Math.max(1, Math.ceil(totalLogs.value / pageSize.value)));
  void loadUsage(false);
}

function changePageSize(size: number): void {
  pageSize.value = size;
  page.value = 1;
  void loadUsage(false);
}

function shouldAutoRefresh(): boolean {
  return props.active
    && range.value === "today"
    && !loading.value
    && !silentLoadInFlight
    && !pricingModalVisible.value;
}

function scheduleAutoRefresh(): void {
  if (autoRefreshTimer) window.clearInterval(autoRefreshTimer);
  autoRefreshTimer = undefined;
  if (autoRefreshIntervalMs.value <= 0) return;
  autoRefreshTimer = window.setInterval(() => {
    if (!shouldAutoRefresh()) return;
    void loadUsage(false, { silent: true });
  }, autoRefreshIntervalMs.value);
}

function refreshPresetAtBoundary(): boolean {
  const nextKey = localDayKey(new Date());
  if (nextKey === presetBoundaryKey) return false;
  presetBoundaryKey = nextKey;
  if (range.value === "custom") return false;
  page.value = 1;
  syncPickerToPreset(range.value);
  if (!props.active || document.visibilityState === "hidden") {
    presetRefreshPending = true;
  } else {
    if (loading.value || silentLoadInFlight) {
      presetRefreshPending = true;
    } else {
      void loadUsage(false, { silent: true });
    }
  }
  return true;
}

function schedulePresetBoundaryRefresh(): void {
  if (presetBoundaryTimer) window.clearTimeout(presetBoundaryTimer);
  const now = Date.now();
  const nextMidnight = nextCalendarDayStart(now) + 150;
  presetBoundaryTimer = window.setTimeout(() => {
    presetBoundaryTimer = undefined;
    refreshPresetAtBoundary();
    schedulePresetBoundaryRefresh();
  }, Math.max(250, nextMidnight - now));
}

function scheduleActivationRefresh(delay = 280): void {
  if (activationRefreshTimer) window.clearTimeout(activationRefreshTimer);
  activationRefreshTimer = window.setTimeout(() => {
    activationRefreshTimer = undefined;
    window.requestAnimationFrame(() => refreshWhenVisible(true));
  }, delay);
}

function refreshWhenVisible(force = false): void {
  const boundaryChanged = refreshPresetAtBoundary();
  if (boundaryChanged && silentLoadInFlight) return;
  if (loading.value || silentLoadInFlight) return;
  if (!props.active || document.visibilityState === "hidden") return;
  const stale = Date.now() - lastLoadedAt.value > 5_000;
  if (!force && !stale && !presetRefreshPending) return;
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

function readTokenDisplayUnit() {
  try {
    return normalizeTokenDisplayUnit(window.localStorage.getItem(tokenDisplayStorageKey));
  } catch {
    return normalizeTokenDisplayUnit(null);
  }
}

watch(tokenDisplayUnit, (unit) => {
  try {
    window.localStorage.setItem(tokenDisplayStorageKey, unit);
  } catch {
    // Display selection still works when local storage is unavailable.
  }
});

function formatTokens(value?: number): string {
  return formatUsageTokens(value, tokenDisplayUnit.value, currentLocale());
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
  const weekday = formatAppDateTime(date, { weekday: "short" });
  const parts = calendarParts(date);
  return `${parts.month}/${parts.day} ${weekday}`;
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
  const date = new Date(timestamp * 1000);
  const today = calendarDateKey(date) === calendarDateKey(new Date());
  return formatAppDateTime(date, {
    month: today ? undefined : "2-digit",
    day: today ? undefined : "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: today ? "2-digit" : undefined,
    hour12: false,
  });
}

function compactAxisNumber(value: number): string {
  return formatTokens(value);
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
  if (disposed || !props.active || !chartContainer.value) return null;
  const core = await loadEchartsApi();
  const container = chartContainer.value;
  // A hidden panel has no usable dimensions. Initialize only after it is shown.
  if (disposed || !props.active || !container?.clientWidth || !container.clientHeight) return null;
  if (!usageChart.value) {
    usageChart.value = core.init(container, undefined, { renderer: "canvas" });
    chartResizeObserver = new ResizeObserver(() => {
      if (props.active && container.clientWidth && container.clientHeight) usageChart.value?.resize();
    });
    chartResizeObserver.observe(container);
  }
  return usageChart.value;
}

function renderUsageChart(): void {
  void nextTick(async () => {
    if (disposed) return;
    if (!trends.value.length) {
      chartResizeObserver?.disconnect();
      chartResizeObserver = null;
      usageChart.value?.dispose();
      usageChart.value = null;
      return;
    }
    if (!props.active) return;
    const core = await loadEchartsApi();
    if (!trends.value.length) return;
    const chart = await ensureUsageChart();
    chart?.resize();
    chart?.setOption(usageChartOption(core), true);
  });
}

onMounted(() => {
  presetBoundaryKey = localDayKey(new Date());
  syncPickerToPreset("today");
  window.setTimeout(() => {
    void loadUsage(false);
    renderUsageChart();
  }, props.active ? 120 : 0);
  scheduleAutoRefresh();
  schedulePresetBoundaryRefresh();
  window.addEventListener("focus", handleWindowFocus);
  document.addEventListener("visibilitychange", handleVisibilityChange);
});

watch([trends, isHourlyTrend, currentLanguage, effectiveTimeZone, tokenDisplayUnit], renderUsageChart, { deep: true });

watch(effectiveTimeZone, (_next, previous) => {
  if (range.value === "custom" && dateRange.value) {
    // Keep the selected instants when only their display zone changes.
    const start = parseWallTime(dateRange.value[0], previous);
    const end = parseWallTime(dateRange.value[1], previous);
    if (start !== null && end !== null) dateRange.value = [formatPickerValue(new Date(start)), formatPickerValue(new Date(end))];
  } else {
    syncPickerToPreset(range.value);
  }
  presetBoundaryKey = localDayKey(new Date());
  schedulePresetBoundaryRefresh();
  page.value = 1;
  void loadUsage(false, { notify: false });
});

watch(
  () => props.instances.map((instance) => instance.id).join("\u0000"),
  () => {
    if (!props.instances.length || selectedUsageInstanceId.value === allInstancesScope) return;
    if (props.instances.some((instance) => instance.id === selectedUsageInstanceId.value)) return;
    selectedUsageInstanceId.value =
      props.instances.find((instance) => instance.id === props.currentInstanceId)?.id
      ?? props.instances.find((instance) => instance.isDefault)?.id
      ?? DEFAULT_CODEX_INSTANCE_ID;
    persistUsageInstanceScope(selectedUsageInstanceId.value);
    page.value = 1;
    if (lastLoadedAt.value > 0) void loadUsage(false, { notify: false });
  },
  { immediate: true },
);

watch(
  () => props.active,
  (active) => {
    if (active) {
      renderUsageChart();
      scheduleActivationRefresh();
    } else if (activationRefreshTimer) {
      window.clearTimeout(activationRefreshTimer);
      activationRefreshTimer = undefined;
    }
  },
);

onBeforeUnmount(() => {
  disposed = true;
  presetRefreshPending = false;
  loadSerial += 1;
  if (autoRefreshTimer) window.clearInterval(autoRefreshTimer);
  if (activationRefreshTimer) window.clearTimeout(activationRefreshTimer);
  if (presetBoundaryTimer) window.clearTimeout(presetBoundaryTimer);
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
        <a-select v-model="tokenDisplayUnit" size="mini" class="usage-token-unit" :aria-label="t('Token 显示单位')">
          <a-option value="compact">K / M / B</a-option>
          <a-option value="localized">{{ t("万 / 亿") }}</a-option>
          <a-option value="full">{{ t("完整数字") }}</a-option>
        </a-select>
        <a-select
          v-if="hasManagedInstances"
          :model-value="selectedUsageInstanceId"
          class="usage-instance-select"
          size="mini"
          :placeholder="t('统计范围')"
          @change="changeUsageInstance"
        >
          <template #prefix><icon-apps /></template>
          <a-option :value="allInstancesScope">{{ t("全部实例") }}</a-option>
          <a-option v-for="instance in props.instances" :key="instance.id" :value="instance.id">
            {{ instanceDisplayName(instance) }}
          </a-option>
        </a-select>
        <a-radio-group :model-value="range" type="button" size="mini" @change="changeRange">
          <a-radio v-for="item in rangeOptions" :key="item.value" :value="item.value">
            {{ t(item.label) }}
          </a-radio>
        </a-radio-group>
        <a-range-picker
          :model-value="dateRange"
          :allow-clear="false"
          :placeholder="[t('开始时间'), t('结束时间')]"
          show-time
          format="YYYY/MM/DD HH:mm:ss"
          value-format="YYYY/MM/DD HH:mm:ss"
          class="usage-range-picker"
          size="mini"
          @change="changeCustomRange"
        />
        <a-select
          :model-value="autoRefreshIntervalMs"
          class="usage-auto-refresh-select"
          size="mini"
          @change="changeAutoRefreshInterval"
        >
          <template #prefix><icon-refresh /></template>
          <a-option v-for="item in autoRefreshOptions" :key="item.value" :value="item.value">
            {{ t(item.label) }}
          </a-option>
        </a-select>
        <a-button size="mini" :loading="loading" @click="refreshUsage">
          <template #icon><icon-refresh /></template>
          {{ t("刷新") }}
        </a-button>
      </div>
    </div>

    <a-spin :loading="loading" dot class="usage-dashboard-spin">
      <div class="usage-dashboard-grid">
        <div class="usage-main-column">
          <div class="usage-hero">
            <div class="usage-hero-main">
              <div class="usage-app-mark"><OpenAiIcon /></div>
              <div>
                <div class="usage-token-heading">
                  <span>{{ t("真实消耗 Tokens") }}</span>
                </div>
                <strong :title="formatFullNumber(summary?.realTotalTokens)">{{ formatTokens(summary?.realTotalTokens) }}</strong>
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
              <span><icon-arrow-down /> {{ t("新增输入 Tokens") }}</span>
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
              <a-tooltip
                v-for="cell in activityCells"
                :key="cell.key"
                position="top"
                :mouse-enter-delay="100"
              >
                <template #content>
                  <div class="usage-activity-tooltip">
                    <strong>{{ cell.label }}</strong>
                    <span>{{ t("当日") }} Token <b>{{ formatFullNumber(cell.tokens) }}</b></span>
                    <span>{{ t("累计") }} Token <b>{{ formatFullNumber(cell.value) }}</b></span>
                    <span>{{ t("请求次数") }} <b>{{ formatLocalizedCount(cell.requests, "次") }}</b></span>
                  </div>
                </template>
                <i
                  class="usage-activity-cell"
                  :class="`level-${cell.level}`"
                  :aria-label="`${cell.label}: ${t('当日')} ${formatFullNumber(cell.tokens)} Tokens, ${activityCellTitle(cell)}`"
                />
              </a-tooltip>
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

        <section class="usage-table-card">
          <a-tabs v-model:active-key="activeUsageTab" class="usage-record-tabs" :lazy-load="true">
            <a-tab-pane key="logs" :title="t('请求日志')">
              <div class="usage-card-title">
                <span>{{ formatLocalizedCount(totalLogs, "条记录") }}</span>
                <span>{{ t("本机会话用量记录") }}</span>
              </div>
              <a-table :data="logs" row-key="requestId" :pagination="false" :bordered="false" :scroll="{ x: 1200 }" class="usage-data-table">
                <template #columns>
                  <a-table-column :title="t('时间')" :width="120">
                    <template #cell="{ record }"><span :title="formatWallTime(record.createdAt * 1000, true)">{{ formatTime(record.createdAt) }}</span></template>
                  </a-table-column>
                  <a-table-column :title="t('应用')" :width="100">
                    <template #cell><span class="usage-app-cell"><OpenAiIcon /> Codex</span></template>
                  </a-table-column>
                  <a-table-column :title="t('来源')" :width="130">
                    <template #cell="{ record }"><span :title="record.dataSource">{{ t('会话日志') }}</span></template>
                  </a-table-column>
                  <a-table-column :title="t('模型')" :width="250">
                    <template #cell="{ record }"><code class="usage-model-id" :title="record.model">{{ record.model }}</code></template>
                  </a-table-column>
                  <a-table-column :title="t('新增输入')" align="right" :width="120">
                    <template #cell="{ record }"><span :title="formatFullNumber(record.inputTokens)">{{ formatTokens(record.inputTokens) }}</span></template>
                  </a-table-column>
                  <a-table-column :title="t('输出')" align="right" :width="110">
                    <template #cell="{ record }"><span :title="formatFullNumber(record.outputTokens)">{{ formatTokens(record.outputTokens) }}</span></template>
                  </a-table-column>
                  <a-table-column :title="t('缓存命中')" align="right" :width="130">
                    <template #cell="{ record }"><span :title="formatFullNumber(record.cacheReadTokens)">{{ formatTokens(record.cacheReadTokens) }}</span></template>
                  </a-table-column>
                  <a-table-column :title="t('预估费用')" align="right" :width="120">
                    <template #cell="{ record }"><strong class="usage-cost">{{ formatUsd(record.totalCost) }}</strong></template>
                  </a-table-column>
                  <a-table-column align="right" :width="120">
                    <template #title><span class="usage-speed-title">{{ t('速度') }}<a-tooltip :content="t('会话日志按请求事件时间戳估算，含首字等待；输出不少于 200 Tokens 且耗时不少于 1 秒时显示 ≈ tok/s。缺少可靠时间戳时显示 —。')"><icon-question-circle /></a-tooltip></span></template>
                    <template #cell="{record}"><span class="usage-speed" :title="record.latencyMs ? `${t('估算耗时')} ${(record.latencyMs / 1000).toFixed(2)}s` : undefined">{{ usageRequestSpeed(record) }}</span></template>
                  </a-table-column>
                </template>
                <template #empty><a-empty :description="t('暂无使用记录')" /></template>
              </a-table>
              <a-pagination
                class="usage-pagination"
                :current="page"
                :page-size="pageSize"
                :total="totalLogs"
                :page-size-options="[20, 50, 100, 200]"
                show-total
                show-page-size
                @change="changePage"
                @page-size-change="changePageSize"
              />
            </a-tab-pane>
            <a-tab-pane key="providers" :title="t('来源汇总')">
              <a-table :data="providerStats" row-key="providerId" :pagination="false" :bordered="false" :scroll="{ x: 960 }" class="usage-data-table">
                <template #columns>
                  <a-table-column :title="t('来源')" data-index="providerName" :width="220" />
                  <a-table-column :title="t('请求数')" align="right">
                    <template #cell="{ record }">{{ formatFullNumber(record.requestCount) }}</template>
                  </a-table-column>
                  <a-table-column title="Tokens" align="right">
                    <template #cell="{ record }"><span :title="formatFullNumber(record.totalTokens)">{{ formatTokens(record.totalTokens) }}</span></template>
                  </a-table-column>
                  <a-table-column :title="t('预估费用')" align="right">
                    <template #cell="{ record }">{{ formatUsd(record.totalCost) }}</template>
                  </a-table-column>
                  <a-table-column align="right" :width="110">
                    <template #title><span class="usage-speed-title">{{ t('成功率') }}<a-tooltip :content="t('会话日志仅记录有用量的成功请求，不包含失败请求。')"><icon-question-circle /></a-tooltip></span></template>
                    <template #cell>100%</template>
                  </a-table-column>
                  <a-table-column align="right" :width="140">
                    <template #title><span class="usage-speed-title">{{ t('速度') }}<a-tooltip :content="t('汇总速度为符合条件请求的总输出除以总耗时，不是逐条平均。会话日志估算含首字等待，仅统计输出不少于 200 Tokens、耗时不少于 1 秒的请求。')"><icon-question-circle /></a-tooltip></span></template>
                    <template #cell="{record}"><span class="usage-speed">{{ formatUsageSpeed(record.estSpeedOutputTokens, record.estSpeedDurationMs) }}</span></template>
                  </a-table-column>
                </template>
                <template #empty><a-empty :description="t('暂无来源数据')" /></template>
              </a-table>
            </a-tab-pane>
            <a-tab-pane key="models" :title="t('模型用量')">
              <a-table :data="modelStats" row-key="model" :pagination="{ pageSize: 20, showTotal: true }" :bordered="false" :scroll="{ x: 1080 }" class="usage-data-table">
                <template #columns>
                  <a-table-column :title="t('模型')" :width="280">
                    <template #cell="{ record }"><code class="usage-model-id" :title="record.model">{{ record.model }}</code></template>
                  </a-table-column>
                  <a-table-column :title="t('请求数')" align="right">
                    <template #cell="{ record }">{{ formatFullNumber(record.requestCount) }}</template>
                  </a-table-column>
                  <a-table-column title="Tokens" align="right">
                    <template #cell="{ record }"><span :title="formatFullNumber(record.totalTokens)">{{ formatTokens(record.totalTokens) }}</span></template>
                  </a-table-column>
                  <a-table-column :title="t('预估费用')" align="right">
                    <template #cell="{ record }">{{ formatUsd(record.totalCost) }}</template>
                  </a-table-column>
                  <a-table-column :title="t('单次均价')" align="right">
                    <template #cell="{ record }">{{ formatUsd(record.avgCostPerRequest) }}</template>
                  </a-table-column>
                  <a-table-column align="right" :width="110">
                    <template #title><span class="usage-speed-title">{{ t('成功率') }}<a-tooltip :content="t('会话日志仅记录有用量的成功请求，不包含失败请求。')"><icon-question-circle /></a-tooltip></span></template>
                    <template #cell>100%</template>
                  </a-table-column>
                  <a-table-column align="right" :width="140">
                    <template #title><span class="usage-speed-title">{{ t('速度') }}<a-tooltip :content="t('汇总速度为符合条件请求的总输出除以总耗时，不是逐条平均。会话日志估算含首字等待，仅统计输出不少于 200 Tokens、耗时不少于 1 秒的请求。')"><icon-question-circle /></a-tooltip></span></template>
                    <template #cell="{record}"><span class="usage-speed">{{ formatUsageSpeed(record.estSpeedOutputTokens, record.estSpeedDurationMs) }}</span></template>
                  </a-table-column>
                </template>
                <template #empty><a-empty :description="t('暂无模型用量')" /></template>
              </a-table>
            </a-tab-pane>
            <a-tab-pane key="pricing" :title="t('定价')">
              <a-spin :loading="pricingLoading" dot class="usage-pricing-spin">
                <div class="usage-pricing-overview">
                  <UsagePricingSync />
                  <section class="usage-pricing-settings usage-pricing-card">
                    <div class="usage-pricing-card-head">
                      <div class="usage-pricing-card-heading">
                        <span class="usage-pricing-card-icon"><icon-settings /></span>
                        <strong>{{ t("计费设置") }}</strong>
                        <span class="usage-pricing-badge">Codex</span>
                      </div>
                      <span v-if="pricingConfigDirty" class="usage-pricing-draft">{{ t("未保存") }}</span>
                    </div>
                    <p class="usage-pricing-description">{{ t("设置统计倍率与模型识别来源") }}</p>
                    <div v-for="item in pricingConfigs" :key="item.app" class="usage-billing-fields">
                      <div class="usage-billing-field">
                        <span>{{ t("默认倍率") }}</span>
                        <a-input v-model="item.multiplier" inputmode="decimal" placeholder="1" :aria-label="t('默认倍率')">
                          <template #suffix>×</template>
                        </a-input>
                      </div>
                      <div class="usage-billing-field">
                        <span>{{ t("计费模式") }}</span>
                        <a-select v-model="item.pricingModelSource" :aria-label="t('计费模式')">
                          <a-option value="response">{{ t("返回模型") }}</a-option>
                          <a-option value="request">{{ t("请求模型") }}</a-option>
                        </a-select>
                      </div>
                    </div>
                    <div class="usage-pricing-card-footer">
                      <span class="usage-pricing-hint">{{ t("按模型单价 × 倍率估算费用") }}</span>
                      <a-button size="small" type="primary" :disabled="!pricingConfigDirty" :loading="savingPricingConfig" @click="savePricingConfig">{{ t("保存") }}</a-button>
                    </div>
                  </section>
                </div>
                <section class="usage-pricing-catalog usage-pricing-card">
                  <div class="usage-card-title usage-pricing-toolbar">
                    <div class="usage-pricing-catalog-heading">
                      <div><strong>{{ t("模型价格库") }}</strong><span class="usage-pricing-count">{{ formatLocalizedCount(pricingList.length, "条规则") }}</span></div>
                      <span>{{ t("USD / 每百万 Tokens") }}</span>
                    </div>
                    <div class="usage-pricing-actions">
                      <a-input-search v-model="pricingSearch" size="small" :placeholder="t('搜索模型 ID 或名称')" allow-clear class="usage-pricing-search" />
                      <a-popconfirm :content="t('恢复内置模型单价会覆盖当前自定义价格，确定继续？')" :ok-text="t('确认')" :cancel-text="t('取消')" @ok="restoreDefaultPricing">
                        <a-button size="small" :loading="pricingLoading"><template #icon><icon-refresh /></template>{{ t("恢复默认") }}</a-button>
                      </a-popconfirm>
                      <a-button size="small" type="primary" @click="openAddPricing"><template #icon><icon-plus /></template>{{ t("添加") }}</a-button>
                    </div>
                  </div>
                  <a-table :data="filteredPricing" row-key="modelId" :pagination="{ pageSize: 20, showPageSize: true, showTotal: true }" :bordered="false" :scroll="{ x: 1080 }" class="usage-data-table usage-pricing-table">
                    <template #columns>
                      <a-table-column :title="t('模型')" :width="330">
                        <template #cell="{ record }"><div class="usage-price-model"><strong :title="record.displayName || record.modelId">{{ record.displayName || record.modelId }}</strong><code class="usage-model-id" :title="record.modelId">{{ record.modelId }}</code></div></template>
                      </a-table-column>
                      <a-table-column :title="t('输入单价')" align="right"><template #cell="{ record }">{{ formatUnitPrice(record.inputCostPerMillion) }}</template></a-table-column>
                      <a-table-column :title="t('输出单价')" align="right"><template #cell="{ record }">{{ formatUnitPrice(record.outputCostPerMillion) }}</template></a-table-column>
                      <a-table-column :title="t('缓存命中')" align="right"><template #cell="{ record }">{{ formatUnitPrice(record.cacheReadCostPerMillion) }}</template></a-table-column>
                      <a-table-column :title="t('缓存写入')" align="right"><template #cell="{ record }">{{ formatUnitPrice(record.cacheCreationCostPerMillion) }}</template></a-table-column>
                      <a-table-column :title="t('操作')" :width="100" align="right">
                        <template #cell="{ record }">
                          <a-space>
                            <a-button size="mini" :aria-label="t('编辑')" @click="openEditPricing(record)"><template #icon><icon-edit /></template></a-button>
                            <a-popconfirm :content="t('确定删除这条模型单价？')" :ok-text="t('确认')" :cancel-text="t('取消')" @ok="removePricing(record.modelId)">
                              <a-button size="mini" status="danger" :aria-label="t('删除')"><template #icon><icon-delete /></template></a-button>
                            </a-popconfirm>
                          </a-space>
                        </template>
                      </a-table-column>
                    </template>
                    <template #empty><a-empty :description="t('暂无模型单价')" /></template>
                  </a-table>
                  <p class="usage-pricing-source"><icon-info-circle /> {{ t("预置价格参考 CC Switch，实际费用以服务商账单为准。") }}</p>
                </section>
              </a-spin>
            </a-tab-pane>
          </a-tabs>
        </section>

      </div>
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
