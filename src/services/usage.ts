import { invoke } from "@tauri-apps/api/core";

export interface CodexUsageDashboard {
  summary: CodexUsageSummary;
  trends: CodexUsageTrendPoint[];
  logs: CodexUsageLog[];
  totalLogs: number;
  providerStats: CodexUsageProviderStat[];
  modelStats: CodexUsageModelStat[];
  filesScanned: number;
  errors: string[];
  cachePath: string;
  pricingConfigs: CodexUsagePricingConfig[];
}

export interface CodexUsageSummary {
  totalRequests: number;
  totalCost: string;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCacheReadTokens: number;
  totalCacheCreationTokens: number;
  realTotalTokens: number;
  cacheHitRate: number;
}

export interface CodexUsageTrendPoint {
  timestamp: number;
  label: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  totalCost: string;
}

export interface CodexUsageLog {
  requestId: string;
  providerName: string;
  model: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  totalCost: string;
  statusCode: number;
  createdAt: number;
  dataSource: string;
}

export interface CodexUsageModelStat {
  model: string;
  requestCount: number;
  totalTokens: number;
  totalCost: string;
  avgCostPerRequest: string;
}

export interface CodexUsageProviderStat {
  providerId: string;
  providerName: string;
  requestCount: number;
  totalTokens: number;
  totalCost: string;
  successRate: number;
  avgLatencyMs: number;
}

export interface CodexUsageQuery {
  startDate?: number | null;
  endDate?: number | null;
  page?: number;
  pageSize?: number;
  refresh?: boolean;
}

export interface CodexUsagePricing {
  modelId: string;
  displayName: string;
  inputCostPerMillion: string;
  outputCostPerMillion: string;
  cacheReadCostPerMillion: string;
  cacheCreationCostPerMillion: string;
}

export interface CodexUsagePricingConfig {
  app: string;
  multiplier: string;
  pricingModelSource: "request" | "response" | string;
}

export function getCodexUsageDashboard(
  query: CodexUsageQuery = {},
): Promise<CodexUsageDashboard> {
  return invoke("codex_get_usage_dashboard", {
    startDate: query.startDate ?? null,
    endDate: query.endDate ?? null,
    page: query.page ?? 1,
    pageSize: query.pageSize ?? 20,
    refresh: query.refresh ?? false,
  });
}

export function listModelPricing(): Promise<CodexUsagePricing[]> {
  return invoke("codex_list_model_pricing");
}

export function updateModelPricing(pricing: CodexUsagePricing): Promise<void> {
  return invoke("codex_update_model_pricing", { pricing });
}

export function deleteModelPricing(modelId: string): Promise<void> {
  return invoke("codex_delete_model_pricing", { modelId });
}

export function resetModelPricing(): Promise<CodexUsagePricing[]> {
  return invoke("codex_reset_model_pricing");
}

export function getPricingConfig(): Promise<CodexUsagePricingConfig[]> {
  return invoke("codex_get_pricing_config");
}

export function updatePricingConfig(
  configs: CodexUsagePricingConfig[],
): Promise<CodexUsagePricingConfig[]> {
  return invoke("codex_update_pricing_config", { configs });
}
