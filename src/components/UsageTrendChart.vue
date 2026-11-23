<script setup lang="ts">
import type { ECharts, EChartsCoreOption } from "echarts/core";
import { onBeforeUnmount, ref, watch } from "vue";
import { currentLanguage, currentLocale, formatLocalizedNumber, t } from "../i18n";
import { formatUsageTokens, type TokenDisplayUnit } from "../usageDisplay";

type TrendPoint = {
  label: string;
  caption?: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  totalCost: number | string | null;
  tokens?: number;
  requests?: number;
  cacheHitRate?: number | null;
};
type TrendField = Exclude<keyof TrendPoint, "label" | "caption">;
type Series = { field: TrendField; label: string; color: string; width: number };
type Core = typeof import("echarts/core");
type TooltipItem = { dataIndex: number; marker?: string; seriesName: string; value: unknown };

const props = withDefaults(defineProps<{
  points: TrendPoint[];
  hourly: boolean;
  active: boolean;
  unit: TokenDisplayUnit;
  metric?: "breakdown" | "tokens" | "requests" | "totalCost" | "cacheHitRate";
}>(), { metric: "breakdown" });

const breakdown: Series[] = [
  { field: "totalCost", label: "预估费用", color: "#ef4444", width: 2.5 },
  { field: "cacheCreationTokens", label: "缓存写入", color: "#f97316", width: 2.5 },
  { field: "cacheReadTokens", label: "缓存复用", color: "#a855f7", width: 3.5 },
  { field: "inputTokens", label: "输入", color: "#3b82f6", width: 3 },
  { field: "outputTokens", label: "输出", color: "#22c55e", width: 3 },
];
const totals: Record<Exclude<typeof props.metric, "breakdown">, Series> = {
  tokens: { field: "tokens", label: "总 Tokens", color: "#3b82f6", width: 3 },
  requests: { field: "requests", label: "总请求数", color: "#22c55e", width: 3 },
  totalCost: breakdown[0],
  cacheHitRate: { field: "cacheHitRate", label: "缓存命中率", color: "#a855f7", width: 3 },
};
const container = ref<HTMLDivElement | null>(null);
const chartUnavailable = ref(false);
let corePromise: Promise<Core> | undefined;
let chart: ECharts | undefined;
let observer: ResizeObserver | undefined;
let renderVersion = 0;

function loadCore(): Promise<Core> {
  corePromise ??= Promise.all([
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
    return core;
  }).catch((cause) => {
    corePromise = undefined;
    throw cause;
  });
  return corePromise;
}

function tooltip(core: Core, series: Series[], params: unknown): string {
  const items = (Array.isArray(params) ? params : [params]) as TooltipItem[];
  const point = props.points[items[0]?.dataIndex];
  const rows = items.map((item) => {
    const field = series.find(({ label }) => t(label) === item.seriesName)?.field;
    const value = item.value;
    const formatted = typeof value !== "number" || !Number.isFinite(value) ? "—"
      : field === "totalCost" ? `$${value.toFixed(value >= 10 ? 2 : 4)}`
        : field === "cacheHitRate" ? `${(value * 100).toFixed(1)}%`
          : formatLocalizedNumber(value);
    return `<div class="usage-echart-tooltip-row">${item.marker ?? ""}<span>${core.format.encodeHTML(item.seriesName)}</span><strong>${formatted}</strong></div>`;
  }).join("");
  return `<div class="usage-echart-tooltip"><strong>${core.format.encodeHTML(point?.caption ?? point?.label ?? "--")}</strong>${rows}</div>`;
}

function chartOption(core: Core): EChartsCoreOption {
  const combined = props.metric === "breakdown";
  const series = props.metric === "breakdown" ? breakdown : [totals[props.metric]];
  const axisNumber = (value: number) => formatUsageTokens(value, props.unit, currentLocale());
  const axisCost = (value: number) => `$${Number(value.toPrecision(3))}`;
  return {
    color: series.map(({ color }) => color),
    animationDuration: 900,
    animationEasing: "cubicOut",
    grid: {
      top: 28,
      right: combined ? 48 : 28,
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
        lineStyle: { color: "rgba(100, 116, 139, 0.38)", width: 1 },
      },
      formatter: (params: unknown) => tooltip(core, series, params),
    },
    legend: {
      bottom: 10,
      left: "center",
      itemWidth: 18,
      itemHeight: 8,
      icon: "roundRect",
      textStyle: { color: "#64748b", fontSize: 12, fontWeight: 600 },
      data: series.map(({ label }) => t(label)),
    },
    xAxis: {
      type: "category",
      boundaryGap: false,
      data: props.points.map(({ label }) => label),
      axisTick: { show: false },
      axisLine: { lineStyle: { color: "rgba(85, 113, 156, 0.18)" } },
      axisLabel: {
        interval: props.hourly ? 0 : "auto",
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
        max: props.metric === "cacheHitRate" ? 1 : undefined,
        minInterval: props.metric === "requests" ? 1 : undefined,
        splitNumber: 5,
        axisLabel: {
          color: "#8a97aa",
          formatter: props.metric === "totalCost" ? axisCost
            : props.metric === "cacheHitRate" ? (value: number) => `${Math.round(value * 100)}%`
              : axisNumber,
        },
        splitLine: { lineStyle: { color: "rgba(85, 113, 156, 0.16)", type: "dashed" } },
      },
      ...(combined ? [{
        type: "value",
        min: 0,
        axisLabel: { color: "#8a97aa", formatter: axisCost },
        splitLine: { show: false },
      }] : []),
    ],
    series: series.map(({ field, label, width }) => ({
      name: t(label),
      type: "line",
      yAxisIndex: combined && field === "totalCost" ? 1 : 0,
      smooth: 0.42,
      showSymbol: props.points.length === 1,
      symbol: "circle",
      symbolSize: 7,
      data: props.points.map((point) => point[field] == null ? null : Number(point[field])),
      lineStyle: { width, type: field === "totalCost" ? "dashed" : "solid" },
      areaStyle: field === "cacheReadTokens" ? {
        color: new core.graphic.LinearGradient(0, 0, 0, 1, [
          { offset: 0, color: "rgba(168, 85, 247, 0.24)" },
          { offset: 1, color: "rgba(168, 85, 247, 0)" },
        ]),
      } : undefined,
      emphasis: { focus: "series" },
    })),
  };
}

function disposeChart(): void {
  observer?.disconnect();
  observer = undefined;
  chart?.dispose();
  chart = undefined;
}

async function renderChart(): Promise<void> {
  const version = ++renderVersion;
  const element = container.value;
  if (!element) {
    disposeChart();
    return;
  }
  if (!props.active) return;
  try {
    const core = await loadCore();
    if (version !== renderVersion || !element.clientWidth || !element.clientHeight) return;
    if (!chart) {
      chart = core.init(element, undefined, { renderer: "canvas" });
      observer = new ResizeObserver(() => {
        if (props.active && element.clientWidth && element.clientHeight) chart?.resize();
      });
      observer.observe(element);
    }
    chartUnavailable.value = false;
    chart.resize();
    chart.setOption(chartOption(core), true);
  } catch {
    if (version === renderVersion) chartUnavailable.value = true;
  }
}

watch([container, () => props.points, () => props.active, () => props.hourly, () => props.metric, () => props.unit,
  currentLanguage], renderChart, { flush: "post" });
onBeforeUnmount(() => {
  renderVersion += 1;
  disposeChart();
});
</script>

<template>
  <div v-if="points.length" class="usage-chart">
    <div ref="container" class="usage-echart" role="img" :aria-label="t('时段消耗曲线')" />
    <a-empty v-if="chartUnavailable" :description="t('无法显示用量曲线')" />
  </div>
  <a-empty v-else :description="t('暂无趋势数据')" />
</template>
