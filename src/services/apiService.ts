import { invoke } from "@tauri-apps/api/core";
import type { CodexAccount } from "../types/codex";

export const API_SERVICE_DOWNLOAD_PROGRESS_EVENT = "codex-switcher-api-service-download-progress";
export const API_SERVICE_AUTO_UPDATE_EVENT = "codex-switcher-api-service-auto-update";

export interface ApiServiceSettings {
  enabled: boolean;
  port: number;
  managementKey: string;
  apiKeys: string[];
  autoUpdate: boolean;
  autoUpdateIntervalHours: number;
  lastUpdateCheckAt?: number | null;
}

export interface ApiServiceRuntime {
  id: string;
  version: string;
  target: string;
  compatible: boolean;
  path: string;
  binaryPath: string;
  installedAt: number;
  packageFile: string;
}

export interface ApiServiceInfo {
  running: boolean;
  pid?: number | null;
  port: number;
  managementUrl: string;
}

export interface ApiServiceState {
  baseDir: string;
  runtimeDir: string;
  workspaceDir: string;
  downloadsDir: string;
  authDir: string;
  settings: ApiServiceSettings;
  activeVersion?: string | null;
  runtimes: ApiServiceRuntime[];
  service: ApiServiceInfo;
  configPath: string;
  installed: boolean;
  maintenanceOldRuntimeCount?: number | null;
}

export interface ApiServiceUpdateInfo {
  currentVersion?: string | null;
  latestVersion: string;
  target: string;
  releaseUrl: string;
  downloadUrl?: string | null;
  assetName?: string | null;
  hasUpdate: boolean;
  canApply: boolean;
  latestInstalled: boolean;
  latestActive: boolean;
}

export interface ApiServiceAutoUpdateEvent {
  status: "checked" | "updated" | "failed" | string;
  updateInfo?: ApiServiceUpdateInfo | null;
  message?: string | null;
}

export type ApiServiceDownloadStatus =
  | "starting"
  | "downloading"
  | "installing"
  | "done"
  | "cancelled"
  | "failed";

export interface ApiServiceDownloadProgress {
  status: ApiServiceDownloadStatus | string;
  assetName: string;
  downloadedBytes: number;
  totalBytes?: number | null;
  message?: string | null;
}

export interface ApiServiceAccountSyncSummary {
  count: number;
  authDir: string;
  oauthCount: number;
  apiKeyCount: number;
}

export interface ApiServiceBoundAccount {
  id: string;
  accountId?: string | null;
  accountIdExact: boolean;
  kind: "oauth" | "apikey" | string;
  label: string;
  email?: string | null;
  baseUrl?: string | null;
  path: string;
  modifiedAt?: number | null;
}

export function isCurrentApiServiceAccount(
  account: CodexAccount,
  serviceState: Pick<ApiServiceState, "settings"> | null | undefined,
): boolean {
  const apiKey = (account.openai_api_key || account.openaiApiKey || "").trim();
  const isApiKey = account.auth_mode === "apikey" || Boolean(apiKey);
  if (!isApiKey || !serviceState) return false;
  const baseUrl = (account.api_base_url || account.apiBaseUrl || "https://api.openai.com/v1").trim();
  try {
    const parsed = new URL(baseUrl);
    const servicePort = Number(serviceState.settings.port);
    const accountPort = Number(parsed.port || (parsed.protocol === "https:" ? 443 : 80));
    if (!servicePort || accountPort !== servicePort) return false;
    const hostname = parsed.hostname.replace(/^\[|\]$/g, "").toLowerCase();
    const localHost =
      hostname === "localhost" ||
      hostname === "::1" ||
      hostname === "::" ||
      hostname === "0.0.0.0" ||
      hostname.startsWith("127.");
    const localKey = Boolean(apiKey && serviceState.settings.apiKeys.some((key) => key.trim() === apiKey));
    return localHost || localKey;
  } catch {
    return false;
  }
}

export function getApiServiceState(): Promise<ApiServiceState> {
  return invoke("api_service_state");
}

export function updateApiServiceSettings(input: {
  port: number;
  managementKey: string;
  apiKeys: string[];
  autoUpdate: boolean;
  autoUpdateIntervalHours: number;
}): Promise<ApiServiceState> {
  return invoke("api_service_update_settings", input);
}

export function startApiService(): Promise<ApiServiceState> {
  return invoke("api_service_start");
}

export function stopApiService(): Promise<ApiServiceState> {
  return invoke("api_service_stop");
}

export function resetApiService(): Promise<ApiServiceState> {
  return invoke("api_service_reset");
}

export function checkApiServiceUpdate(): Promise<ApiServiceUpdateInfo> {
  return invoke("api_service_check_update");
}

export function downloadApiServiceUpdate(): Promise<ApiServiceState> {
  return invoke("api_service_download_update");
}

export function importApiServiceRuntime(packagePath: string): Promise<ApiServiceState> {
  return invoke("api_service_import_runtime", { packagePath });
}

export function activateApiServiceRuntime(runtimeId: string): Promise<ApiServiceState> {
  return invoke("api_service_activate_runtime", { runtimeId });
}

export function deleteApiServiceRuntime(runtimeId: string): Promise<ApiServiceState> {
  return invoke("api_service_delete_runtime", { runtimeId });
}

export function cancelApiServiceDownload(): Promise<void> {
  return invoke("api_service_cancel_download");
}

export function bindApiServiceAccounts(accountIds: string[]): Promise<ApiServiceAccountSyncSummary> {
  return invoke("api_service_bind_accounts", { accountIds });
}

export function listApiServiceBoundAccounts(): Promise<ApiServiceBoundAccount[]> {
  return invoke("api_service_list_bound_accounts");
}

export function deleteApiServiceBoundAccounts(boundIds: string[]): Promise<ApiServiceAccountSyncSummary> {
  return invoke("api_service_delete_bound_accounts", { boundIds });
}

export function deleteApiServiceAccountBinding(accountId: string): Promise<ApiServiceAccountSyncSummary> {
  return invoke("api_service_delete_account_binding", { accountId });
}

/** API 服务接入 Codex 实例（同步 / 恢复）的进度事件；payload 为 { step }。 */
export const API_SERVICE_CODEX_SYNC_PROGRESS_EVENT = "codex-switcher-api-service-codex-sync-progress";

export interface ApiServiceCodexSyncProgress {
  /** 0 启动 API 服务，1 关闭实例并同步配置，2 修复切号会话，3 恢复全部完整历史，4 重新打开实例 */
  step: number;
}

export interface ApiServiceCodexSyncSummary {
  instanceId: string;
  instanceName: string;
  accountId: string;
  /** 自动绑定到 API 服务账号上的 OAuth 账号 ID（没有可用账号时为 null）。 */
  boundOauthAccountId: string | null;
  baseUrl: string;
  serviceStarted: boolean;
  sessionCount: number;
  repairMessage: string;
  message: string;
}

export interface ApiServiceCodexRestoreSummary {
  instanceId: string;
  instanceName: string;
  synchronizedSessionProviderCount: number;
  message: string;
}

/** 同步配置：确保服务运行 → 停止实例 → 重置基础配置并写入 API 服务路由 → 一键修复会话 → 重新打开实例。 */
export function syncApiServiceCodexInstance(instanceId: string): Promise<ApiServiceCodexSyncSummary> {
  return invoke("api_service_sync_codex_instance", { instanceId });
}

/** 恢复配置：停止实例 → 重置为基础配置（移除 API 服务路由）→ 同步旧会话 provider → 重新打开实例。 */
export function restoreApiServiceCodexInstance(instanceId: string): Promise<ApiServiceCodexRestoreSummary> {
  return invoke("api_service_restore_codex_instance", { instanceId });
}
