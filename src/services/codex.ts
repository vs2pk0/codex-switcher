import { invoke } from "@tauri-apps/api/core";
import type { CodexAccount } from "../types/codex";
import type { CodexSessionVisibilityRepairSummary } from "./session";

export type CodexExportFormat = "cockpit_tools" | "sub2api" | "cpa";

export interface CodexOAuthLoginStartResponse {
  loginId: string;
  authUrl: string;
}

export interface CodexResetCreditConsumeResult {
  quotaRefreshError?: string;
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
  accountViewMode: "card" | "compact" | "table";
  sidebarEnabled: boolean;
  showQuotaCountdowns: boolean;
  showAdditionalQuotaWindows: boolean;
  badgeStyle: string;
  badgeStyles: Record<string, string>;
  maxColumns: 3 | 4 | 5;
  language: string;
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

export type CodexConfigFileKind = "auth" | "config";

export interface CodexConfigFileContent {
  kind: CodexConfigFileKind;
  name: "auth.json" | "config.toml";
  path: string;
  content: string;
  exists: boolean;
}

export interface CodexApiKeyModel {
  id: string;
  ownedBy?: string;
}

export interface CodexApiKeyBalance {
  provider: string;
  balanceKind: "wallet" | "key_quota" | "subscription" | "unlimited";
  availableAmount?: number | null;
  usedAmount?: number | null;
  totalAmount?: number | null;
  currency: string;
  unlimited: boolean;
  planName?: string | null;
}

export type CodexApiKeyBalanceStatus = "loading" | "success" | "error" | "consent_required";

export interface CodexApiKeyBalanceState {
  status: CodexApiKeyBalanceStatus;
  balance?: CodexApiKeyBalance;
  error?: string;
  fetchedAt: number;
}

export interface CodexSwitcherBackupFile {
  name: string;
  path: string;
  createdAt: string;
  sizeBytes: number;
  sourceInstanceId?: string | null;
  sourceInstanceName?: string | null;
  manual: boolean;
}

export interface CodexSwitcherBackupProgressEvent {
  taskId: string;
  status: "running" | "completed" | "failed";
  progress: number;
  message: string;
  backupFile?: CodexSwitcherBackupFile | null;
}

export interface CodexSessionRestoreResult {
  restored: boolean;
  visibilityRepaired: boolean;
  visibility?: CodexSessionVisibilityRepairSummary | null;
  warning?: string | null;
}

export interface CodexAccountSwitchResult {
  account: CodexAccount;
  synchronizedSessionProviderCount: number;
  warning?: string | null;
}

export interface CodexSessionModelCompatibilityRepairSummary {
  targetProvider: string;
  repairedRolloutFileCount: number;
  rewrittenRolloutModelFieldCount: number;
  synchronizedRolloutProviderCount: number;
  removedEncryptedReasoningItemCount: number;
  removedEncryptedCompactionItemCount: number;
  repairedThreadCount: number;
  synchronizedCatalogRowCount: number;
  repairedDatabaseCount: number;
  backupDirs: string[];
}

export async function reloadCodexAfterSessionVisibilityRepair(
  summary: Pick<CodexSessionVisibilityRepairSummary, "desktopReloadRequired">,
  restart: () => Promise<string> = restartCodexApp,
): Promise<string | null> {
  if (summary.desktopReloadRequired !== true) return null;
  return restart();
}

export function listCodexAccounts(): Promise<CodexAccount[]> {
  return invoke("list_codex_accounts");
}

export function getCurrentCodexAccount(): Promise<CodexAccount | null> {
  return invoke("get_current_codex_account");
}

export function detectCurrentCodexAccount(): Promise<CodexAccount | null> {
  return invoke("detect_current_codex_account");
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
  accountName?: string;
  tags?: string[];
  isHidden?: boolean;
}): Promise<CodexAccount> {
  return invoke("update_codex_api_key_credentials", {
    input: {
      accountId: input.accountId,
      apiKey: input.apiKey,
      apiBaseUrl: input.apiBaseUrl || null,
      apiProviderName: input.apiProviderName || null,
      apiOfficialUrl: input.apiOfficialUrl || null,
      accountName: input.accountName || null,
      tags: input.tags || [],
      isHidden: Boolean(input.isHidden),
    },
  });
}

export function fetchCodexApiKeyModels(accountId: string): Promise<CodexApiKeyModel[]> {
  return invoke("fetch_codex_api_key_models", { accountId });
}

export function fetchCodexApiKeyBalance(
  accountId: string,
  approvedInsecureHttpOrigin?: string,
): Promise<CodexApiKeyBalance> {
  return invoke("fetch_codex_api_key_balance", {
    accountId,
    approvedInsecureHttpOrigin: approvedInsecureHttpOrigin || null,
  });
}

export function checkCodexApiKeyModelAccess(accountId: string): Promise<boolean> {
  return invoke("check_codex_api_key_model_access", { accountId });
}

export function setCodexApiKeyDefaultModel(input: {
  accountId: string;
  modelId: string;
}): Promise<CodexAccount> {
  return invoke("set_codex_api_key_default_model", {
    accountId: input.accountId,
    modelId: input.modelId,
  });
}

export function updateCodexAccountProfile(input: {
  accountId: string;
  accountName?: string;
  tags?: string[];
  isHidden?: boolean;
}): Promise<CodexAccount> {
  return invoke("update_codex_account_profile", {
    accountId: input.accountId,
    accountName: input.accountName || null,
    tags: input.tags || [],
    isHidden: Boolean(input.isHidden),
  });
}

export function completeCodexHiddenAccountCleanup(accountId: string): Promise<CodexAccount> {
  return invoke("complete_codex_hidden_account_cleanup", { accountId });
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

export function switchCodexAccount(
  accountId: string,
  instanceId = "default",
): Promise<CodexAccountSwitchResult> {
  return invoke("switch_codex_account", { accountId, instanceId });
}

export function restartCodexApp(instanceId = "default"): Promise<string> {
  return invoke("restart_codex_app", { instanceId });
}

export function repairCodexSessionModelCompatibility(
  instanceId = "default",
): Promise<CodexSessionModelCompatibilityRepairSummary> {
  return invoke("repair_codex_session_model_compatibility", { instanceId });
}

export function getCodexSwitcherSettings(): Promise<CodexSwitcherSettings> {
  return invoke("get_codex_switcher_settings");
}

export function updateCodexSwitcherSettings(
  settings: CodexSwitcherSettings,
): Promise<CodexSwitcherSettings> {
  return invoke("update_codex_switcher_settings", { settings });
}

export function getCodexSwitcherPaths(instanceId = "default"): Promise<CodexSwitcherPaths> {
  return invoke("get_codex_switcher_paths", { instanceId });
}

export function readCodexConfigFile(
  fileKind: CodexConfigFileKind,
  instanceId = "default",
): Promise<CodexConfigFileContent> {
  return invoke("read_codex_config_file", { fileKind, instanceId });
}

export function formatCodexConfigFile(
  fileKind: CodexConfigFileKind,
  content: string,
): Promise<string> {
  return invoke("format_codex_config_file", { fileKind, content });
}

export function writeCodexConfigFile(
  fileKind: CodexConfigFileKind,
  content: string,
  instanceId = "default",
): Promise<CodexConfigFileContent> {
  return invoke("write_codex_config_file", { fileKind, content, instanceId });
}

export function exportCodexSwitcherBackup(): Promise<CodexSwitcherBackupFile> {
  return invoke("export_codex_switcher_backup");
}

export function startCodexSwitcherBackup(taskId: string): Promise<string> {
  return invoke("start_codex_switcher_backup", { taskId });
}

export function startCodexSwitcherSessionBackup(
  taskId: string,
  instanceId = "default",
): Promise<string> {
  return invoke("start_codex_switcher_session_backup", { taskId, instanceId });
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

export function restoreCodexSwitcherSessionBackup(
  backupPath: string,
  instanceId = "default",
): Promise<CodexSessionRestoreResult> {
  return invoke("restore_codex_switcher_session_backup", { backupPath, instanceId });
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

export function consumeCodexResetCredit(accountId: string): Promise<CodexResetCreditConsumeResult> {
  return invoke("consume_codex_reset_credit", { accountId });
}

export function refreshAllCodexQuotas(): Promise<number> {
  return invoke("refresh_all_codex_quotas");
}

export function resetCodexConfigToml(instanceId = "default"): Promise<CodexConfigFileContent> {
  return invoke("reset_codex_config_toml", { instanceId });
}

export function deleteCodexConfigToml(instanceId = "default"): Promise<boolean> {
  return invoke("delete_codex_config_toml", { instanceId });
}
