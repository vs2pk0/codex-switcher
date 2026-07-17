import { invoke } from "@tauri-apps/api/core";

export const APP_UPDATE_DOWNLOAD_PROGRESS_EVENT =
  "codex-switcher-app-update-download-progress";

export interface AppUpdateInfo {
  currentVersion: string;
  latestVersion: string;
  releaseUrl: string;
  target: string;
  assetName?: string | null;
  assetSize?: number | null;
  hasUpdate: boolean;
  canDownload: boolean;
}

export interface AppUpdateDownloadProgress {
  status: "checking" | "starting" | "downloading" | "completed" | "cancelled" | "failed" | string;
  version: string;
  assetName: string;
  downloadedBytes: number;
  totalBytes?: number | null;
  message?: string | null;
}

export interface AppUpdateDownloadResult {
  version: string;
  assetName: string;
  path: string;
  sizeBytes: number;
}

export function fetchAppUpdateInfo(): Promise<AppUpdateInfo> {
  return invoke("app_update_check");
}

export function downloadAppUpdate(): Promise<AppUpdateDownloadResult> {
  return invoke("app_update_download");
}

export function cancelAppUpdateDownload(): Promise<void> {
  return invoke("app_update_cancel_download");
}

export function openAppUpdateInstaller(path: string): Promise<void> {
  return invoke("app_update_open_installer", { path });
}
