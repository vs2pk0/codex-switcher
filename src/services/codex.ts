import { invoke } from "@tauri-apps/api/core";
import type { CodexAccount } from "../types/codex";

export type CodexExportFormat = "cockpit_tools" | "sub2api" | "cpa";

export interface CodexOAuthLoginStartResponse {
  loginId: string;
  authUrl: string;
}

export interface CodexSwitcherSettings {
  monitorQuota: boolean;
  quotaRefreshMinutes: number;
  currentAccountRefreshMinutes: number;
  quotaNextRefreshAt?: number;
  currentAccountNextRefreshAt?: number;
  sortMode: string;
  sortDirection: "asc" | "desc";
  customOrder: string[];
  pinnedAccountIds: string[];
  accountTypeFilter: string;
  pageSize: number;
  showQuotaCountdowns: boolean;
  badgeStyle: string;
  badgeStyles: Record<string, string>;
  maxColumns: 3 | 4 | 5;
}

export interface CodexSwitcherPaths {
  appDir: string;
  accountsJson: string;
  settingsJson: string;
  backupDir: string;
  accountDir: string;
  sessionDir: string;
  statisticsDir: string;
  dataDir: string;
  codexHome: string;
}

export interface CodexSwitcherBackupFile {
  name: string;
  path: string;
  createdAt: string;
  sizeBytes: number;
}

export interface CodexSwitcherBackupProgressEvent {
  taskId: string;
  status: "running" | "completed" | "failed";
  progress: number;
  message: string;
  backupFile?: CodexSwitcherBackupFile | null;
}

export function listCodexAccounts(): Promise<CodexAccount[]> {
  return invoke("list_codex_accounts");
}

export function getCurrentCodexAccount(): Promise<CodexAccount | null> {
  return invoke("get_current_codex_account");
}

export function importCodexFromJson(jsonContent: string): Promise<CodexAccount[]> {
  return invoke("import_codex_from_json", { jsonContent });
}

export function importCodexFromLocal(): Promise<CodexAccount[]> {
  return invoke("import_codex_from_local");
}

export function startCodexOAuthLogin(): Promise<CodexOAuthLoginStartResponse> {
  return invoke("codex_oauth_login_start");
}

export function submitCodexOAuthCallbackUrl(input: {
  loginId: string;
  callbackUrl: string;
}): Promise<void> {
  return invoke("codex_oauth_submit_callback_url", {
    loginId: input.loginId,
    callbackUrl: input.callbackUrl,
  });
}

export function completeCodexOAuthLogin(loginId: string): Promise<CodexAccount> {
  return invoke("codex_oauth_login_completed", { loginId });
}

export function cancelCodexOAuthLogin(loginId?: string): Promise<void> {
  return invoke("codex_oauth_login_cancel", { loginId: loginId || null });
}

export function openExternalUrl(url: string): Promise<void> {
  return invoke("open_external_url", { url });
}

export function addCodexAccountWithApiKey(input: {
  apiKey: string;
  apiBaseUrl?: string;
  apiProviderName?: string;
  apiOfficialUrl?: string;
  accountName?: string;
  boundOauthAccountId?: string;
  boundOauthUseLocalGateway?: boolean;
}): Promise<CodexAccount> {
  return invoke("add_codex_account_with_api_key", {
    apiKey: input.apiKey,
    apiBaseUrl: input.apiBaseUrl || null,
    apiProviderName: input.apiProviderName || null,
    apiOfficialUrl: input.apiOfficialUrl || null,
    apiOfficialURL: input.apiOfficialUrl || null,
    api_official_url: input.apiOfficialUrl || null,
    accountName: input.accountName || null,
    boundOauthAccountId: input.boundOauthAccountId || null,
    boundOauthUseLocalGateway: input.boundOauthUseLocalGateway ?? false,
  });
}

export function updateCodexApiKeyCredentials(input: {
  accountId: string;
  apiKey: string;
  apiBaseUrl?: string;
  apiProviderName?: string;
  apiOfficialUrl?: string;
}): Promise<CodexAccount> {
  return invoke("update_codex_api_key_credentials", {
    accountId: input.accountId,
    apiKey: input.apiKey,
    apiBaseUrl: input.apiBaseUrl || null,
    apiProviderName: input.apiProviderName || null,
    apiOfficialUrl: input.apiOfficialUrl || null,
    apiOfficialURL: input.apiOfficialUrl || null,
    api_official_url: input.apiOfficialUrl || null,
  });
}

export function updateCodexAccountProfile(input: {
  accountId: string;
  accountName?: string;
}): Promise<CodexAccount> {
  return invoke("update_codex_account_profile", {
    accountId: input.accountId,
    accountName: input.accountName || null,
  });
}

export function updateCodexAccountFromJson(input: {
  accountId: string;
  jsonContent: string;
}): Promise<CodexAccount> {
  return invoke("update_codex_account_from_json", {
    accountId: input.accountId,
    jsonContent: input.jsonContent,
  });
}

export function updateCodexApiKeyBoundOAuthAccount(input: {
  accountId: string;
  boundOauthAccountId?: string | null;
  boundOauthUseLocalGateway?: boolean;
}): Promise<CodexAccount> {
  return invoke("update_codex_api_key_bound_oauth_account", {
    accountId: input.accountId,
    boundOauthAccountId: input.boundOauthAccountId || null,
    boundOauthUseLocalGateway: input.boundOauthUseLocalGateway ?? false,
  });
}

export function updateCodexAccountPhone(input: {
  accountId: string;
  phone: string;
}): Promise<CodexAccount> {
  return invoke("update_codex_account_phone", {
    accountId: input.accountId,
    phone: input.phone,
  });
}

export function exportCodexAccounts(
  accountIds: string[],
  format: CodexExportFormat = "cockpit_tools",
): Promise<string> {
  return invoke("export_codex_accounts", { accountIds, format });
}

export function deleteCodexAccount(accountId: string): Promise<void> {
  return invoke("delete_codex_account", { accountId });
}

export function switchCodexAccount(accountId: string): Promise<CodexAccount> {
  return invoke("switch_codex_account", { accountId });
}

export function restartCodexApp(): Promise<string> {
  return invoke("restart_codex_app");
}

export function getCodexSwitcherSettings(): Promise<CodexSwitcherSettings> {
  return invoke("get_codex_switcher_settings");
}

export function updateCodexSwitcherSettings(
  settings: CodexSwitcherSettings,
): Promise<CodexSwitcherSettings> {
  return invoke("update_codex_switcher_settings", { settings });
}

export function getCodexSwitcherPaths(): Promise<CodexSwitcherPaths> {
  return invoke("get_codex_switcher_paths");
}

export function exportCodexSwitcherBackup(): Promise<CodexSwitcherBackupFile> {
  return invoke("export_codex_switcher_backup");
}

export function startCodexSwitcherBackup(taskId: string): Promise<string> {
  return invoke("start_codex_switcher_backup", { taskId });
}

export function startCodexSwitcherSessionBackup(taskId: string): Promise<string> {
  return invoke("start_codex_switcher_session_backup", { taskId });
}

export function listCodexSwitcherBackups(): Promise<CodexSwitcherBackupFile[]> {
  return invoke("list_codex_switcher_backups");
}

export function listCodexSwitcherSessionBackups(): Promise<CodexSwitcherBackupFile[]> {
  return invoke("list_codex_switcher_session_backups");
}

export function restoreCodexSwitcherBackup(backupPath: string): Promise<CodexAccount[]> {
  return invoke("restore_codex_switcher_backup", { backupPath });
}

export function restoreCodexSwitcherSessionBackup(backupPath: string): Promise<void> {
  return invoke("restore_codex_switcher_session_backup", { backupPath });
}

export function deleteCodexSwitcherBackup(backupPath: string): Promise<void> {
  return invoke("delete_codex_switcher_backup", { backupPath });
}

export function importCodexSwitcherBackup(jsonContent: string): Promise<CodexAccount[]> {
  return invoke("import_codex_switcher_backup", { jsonContent });
}

export function refreshCodexQuota(accountId: string): Promise<CodexAccount> {
  return invoke("refresh_codex_quota", { accountId });
}

export function consumeCodexResetCredit(accountId: string): Promise<CodexAccount> {
  return invoke("consume_codex_reset_credit", { accountId });
}

export function refreshAllCodexQuotas(): Promise<number> {
  return invoke("refresh_all_codex_quotas");
}

export function resetCodexConfigToml(): Promise<boolean> {
  return invoke("reset_codex_config_toml");
}
