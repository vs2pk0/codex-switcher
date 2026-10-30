export type TokenDisplayUnit = "compact" | "localized" | "full";

export const tokenDisplayStorageKey = "codex-switcher:usage-token-display-unit";

/** CC Switch speed thresholds; session estimates include time waiting for the first token. */
export function usageRequestSpeed(log: { outputTokens: number; latencyMs?: number; firstTokenMs?: number | null; dataSource?: string }): string {
  const estimated = !!log.dataSource && log.dataSource !== "proxy";
  const duration = estimated ? log.latencyMs : log.firstTokenMs == null ? undefined : (log.latencyMs ?? 0) - log.firstTokenMs;
  if (!Number.isFinite(log.outputTokens) || log.outputTokens < (estimated ? 200 : 100) || !duration || !Number.isFinite(duration) || duration < (estimated ? 1000 : 100)) return "—";
  return formatUsageSpeed(log.outputTokens, duration, estimated);
}

export function formatUsageSpeed(tokens?: number, durationMs?: number, estimated = true): string {
  if (!tokens || !durationMs || !Number.isFinite(tokens) || !Number.isFinite(durationMs) || tokens <= 0 || durationMs <= 0) return "—";
  const speed = tokens / (durationMs / 1000);
  return `${estimated ? "≈" : ""}${speed >= 1 ? Math.round(speed) : speed.toFixed(1)} tok/s`;
}

export function formatUnitPrice(value: string): string {
  const number = Number(value);
  if (!Number.isFinite(number)) return `$${value}`;
  return `$${number.toLocaleString("en-US", {
    useGrouping: false,
    minimumFractionDigits: 2,
    maximumFractionDigits: 20,
  })}`;
}

export function normalizeTokenDisplayUnit(value: unknown): TokenDisplayUnit {
  return value === "localized" || value === "full" ? value : "compact";
}

/** Display only: the original token count is always used for totals and pricing. */
export function formatUsageTokens(
  value: number | undefined,
  unit: TokenDisplayUnit,
  locale: string,
): string {
  const count = Number.isFinite(value) ? Math.max(0, value ?? 0) : 0;
  const format = (number: number, digits = 0) =>
    new Intl.NumberFormat(locale, { maximumFractionDigits: digits }).format(number);
  if (unit === "full") return format(count);
  const chinese = locale.startsWith("zh");
  const scales: Array<[number, string]> = unit === "localized" && chinese
    ? [[100_000_000, "亿"], [10_000, "万"]]
    : [[1_000_000_000, "B"], [1_000_000, "M"], [1_000, "K"]];
  for (const [index, [scale, suffix]] of scales.entries()) {
    // Promote values that would otherwise round to 1,000K or 10,000万.
    const smallerScale = scales[index + 1]?.[0] ?? 1;
    if (count >= scale - smallerScale * 0.005) {
      const label = suffix === "亿" && locale === "zh-TW" ? "億" : suffix === "万" && locale === "zh-TW" ? "萬" : suffix;
      return `${format(count / scale, 2)}${label}`;
    }
  }
  return format(count);
}
