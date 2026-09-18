import { invoke } from "@tauri-apps/api/core";

export interface QuotaListWindow {
  key: string;
  label: string;
  percentage: number;
  resetTime?: number;
  windowMinutes?: number;
}

export interface QuotaListAccount {
  accountId: string;
  label: string;
  windows: QuotaListWindow[];
  error?: string;
}

export interface QuotaListResult {
  /** "apiService" | "openCodex" | "remote"。 */
  scope: string;
  available: boolean;
  cachedAt: number;
  accounts: QuotaListAccount[];
}

export interface QuotaListStatus {
  scope: string;
  available: boolean;
}

export interface QuotaListState {
  status: "loading" | "ready" | "error";
  result: QuotaListResult | null;
  error?: string;
}

/** 探测某个 Base URL 背后是否提供额度列表（本地 API 服务 / OpenCodex 或远程接口）。 */
export function checkQuotaListStatus(baseUrl: string): Promise<QuotaListStatus> {
  return invoke<QuotaListStatus>("check_quota_list_status", { baseUrl });
}

/** 查询额度列表；后端缓存 2 分钟，force 用于手动刷新。 */
export function fetchQuotaList(baseUrl: string, force: boolean): Promise<QuotaListResult> {
  return invoke<QuotaListResult>("fetch_quota_list", { baseUrl, force });
}
