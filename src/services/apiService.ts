import { invoke } from "@tauri-apps/api/core";

export const API_SERVICE_DOWNLOAD_PROGRESS_EVENT = "codex-switcher-api-service-download-progress";

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
}

export interface ApiServiceUpdateInfo {
  currentVersion?: string | null;
  latestVersion: string;
  target: string;
  releaseUrl: string;
  downloadUrl?: string | null;
  assetName?: string | null;
  hasUpdate: boolean;
  latestInstalled: boolean;
  latestActive: boolean;
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
}

export interface ApiServiceBoundAccount {
  email: string;
  path: string;
  modifiedAt?: number | null;
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

export function cancelApiServiceDownload(): Promise<void> {
  return invoke("api_service_cancel_download");
}

export function bindApiServiceAccounts(accountIds: string[]): Promise<ApiServiceAccountSyncSummary> {
  return invoke("api_service_bind_accounts", { accountIds });
}

export function listApiServiceBoundAccounts(): Promise<ApiServiceBoundAccount[]> {
  return invoke("api_service_list_bound_accounts");
}

export function deleteApiServiceBoundAccounts(emails: string[]): Promise<ApiServiceAccountSyncSummary> {
  return invoke("api_service_delete_bound_accounts", { emails });
}
