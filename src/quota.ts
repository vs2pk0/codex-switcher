import type { CodexQuota } from "./types/codex";

export type QuotaWindowKind = "hourly" | "weekly";

export interface QuotaWindowSnapshot {
  percentage: number;
  resetTime?: number;
}

export interface AdditionalQuotaWindowSnapshot {
  key: string;
  label: string;
  percentage: number;
  resetTime?: number;
  windowMinutes?: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function isPositiveNumber(value: unknown): boolean {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function finiteNumber(value: unknown): number | undefined {
  const parsed = typeof value === "string" && value.trim() ? Number(value) : value;
  return typeof parsed === "number" && Number.isFinite(parsed) ? parsed : undefined;
}

function recordValue(
  record: Record<string, unknown>,
  snakeCaseKey: string,
  camelCaseKey: string,
): unknown {
  return record[snakeCaseKey] ?? record[camelCaseKey];
}

function additionalRateLimits(quota: CodexQuota): unknown[] {
  if (!isRecord(quota.raw_data)) return [];
  const nestedData = isRecord(quota.raw_data.data) ? quota.raw_data.data : undefined;
  const value =
    recordValue(quota.raw_data, "additional_rate_limits", "additionalRateLimits") ??
    (nestedData
      ? recordValue(nestedData, "additional_rate_limits", "additionalRateLimits")
      : undefined);
  return Array.isArray(value) ? value : [];
}

function additionalLimitLabel(limit: Record<string, unknown>): string {
  const value = recordValue(limit, "limit_name", "limitName");
  return typeof value === "string" ? value.trim().replace(/[-_]+/g, " ") : "";
}

export function additionalQuotaWindows(
  quota: CodexQuota | null | undefined,
): AdditionalQuotaWindowSnapshot[] {
  if (!quota) return [];

  const snapshots: AdditionalQuotaWindowSnapshot[] = [];
  additionalRateLimits(quota).forEach((value, limitIndex) => {
    if (!isRecord(value)) return;
    const label = additionalLimitLabel(value);
    const rateLimitValue = recordValue(value, "rate_limit", "rateLimit");
    if (!label || !isRecord(rateLimitValue)) return;

    (["primary", "secondary"] as const).forEach((slot) => {
      const windowValue = recordValue(
        rateLimitValue,
        `${slot}_window`,
        `${slot}Window`,
      );
      if (!isRecord(windowValue) || Object.keys(windowValue).length === 0) return;

      const usedPercentage = finiteNumber(
        recordValue(windowValue, "used_percent", "usedPercent"),
      );
      const remainingPercentage = finiteNumber(
        recordValue(windowValue, "remaining_percent", "remainingPercent"),
      );
      const percentage = remainingPercentage ??
        (usedPercentage === undefined ? undefined : 100 - usedPercentage);
      if (percentage === undefined) return;

      const windowSeconds = finiteNumber(
        recordValue(windowValue, "limit_window_seconds", "limitWindowSeconds"),
      );
      const resetTime = finiteNumber(recordValue(windowValue, "reset_at", "resetAt"));
      snapshots.push({
        key: `${limitIndex}:${label}:${slot}:${windowSeconds ?? "unknown"}`,
        label,
        percentage: Math.round(Math.max(0, Math.min(100, percentage)) * 10) / 10,
        ...(resetTime === undefined ? {} : { resetTime }),
        ...(windowSeconds && windowSeconds > 0
          ? { windowMinutes: Math.max(1, Math.round(windowSeconds / 60)) }
          : {}),
      });
    });
  });

  return snapshots;
}

function rawWindowPresence(
  quota: CodexQuota,
  kind: QuotaWindowKind,
): boolean | undefined {
  if (!isRecord(quota.raw_data)) return undefined;
  const nestedData = isRecord(quota.raw_data.data) ? quota.raw_data.data : undefined;
  const rateLimit = isRecord(quota.raw_data.rate_limit)
    ? quota.raw_data.rate_limit
    : isRecord(nestedData?.rate_limit)
      ? nestedData.rate_limit
      : undefined;
  if (!rateLimit) return undefined;
  const key = kind === "hourly" ? "primary_window" : "secondary_window";
  if (!(key in rateLimit)) return undefined;
  const window = rateLimit[key];
  return isRecord(window) && Object.keys(window).length > 0;
}

export function hasQuotaWindow(
  quota: CodexQuota | null | undefined,
  kind: QuotaWindowKind,
): boolean {
  if (!quota) return false;
  if (kind === "hourly") {
    if (quota.hourly_window_present === false) return false;
    const rawPresent = rawWindowPresence(quota, kind);
    if (rawPresent !== undefined) return rawPresent;
    return isPositiveNumber(quota.hourly_window_minutes)
      || isPositiveNumber(quota.hourly_reset_time);
  }
  if (quota.weekly_window_present === false) return false;
  const rawPresent = rawWindowPresence(quota, kind);
  if (rawPresent !== undefined) return rawPresent;
  return isPositiveNumber(quota.weekly_window_minutes)
    || isPositiveNumber(quota.weekly_reset_time);
}

export function hasAnyQuotaWindow(quota: CodexQuota | null | undefined): boolean {
  return hasQuotaWindow(quota, "hourly") || hasQuotaWindow(quota, "weekly");
}

export function quotaWindowForMinutes(
  quota: CodexQuota | null | undefined,
  targetMinutes: number,
): QuotaWindowSnapshot | undefined {
  if (!quota) return undefined;
  if (hasQuotaWindow(quota, "hourly") && quota.hourly_window_minutes === targetMinutes) {
    return {
      percentage: quota.hourly_percentage,
      resetTime: quota.hourly_reset_time,
    };
  }
  if (hasQuotaWindow(quota, "weekly") && quota.weekly_window_minutes === targetMinutes) {
    return {
      percentage: quota.weekly_percentage,
      resetTime: quota.weekly_reset_time,
    };
  }
  return undefined;
}
