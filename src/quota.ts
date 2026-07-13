import type { CodexQuota } from "./types/codex";

export type QuotaWindowKind = "hourly" | "weekly";

export interface QuotaWindowSnapshot {
  percentage: number;
  resetTime?: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function isPositiveNumber(value: unknown): boolean {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
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
