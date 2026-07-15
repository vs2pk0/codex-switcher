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
