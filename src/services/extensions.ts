import { invoke } from "@tauri-apps/api/core";

export interface ExtensionPlatform {
  id: string;
  label: string;
  login: "browser" | "browser_callback" | "code";
  loginOptions: string[];
  iconPng?: string;
  localImport?: boolean;
}

export interface InstalledExtension {
  id: string;
  name: string;
  version: string;
  enabled?: boolean;
  accountActions?: boolean;
  usage: boolean;
  usageAppDiscovery?: boolean;
  usageIcons?: Record<string, string>;
  platforms: ExtensionPlatform[];
}

export const usageAppNames: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex",
  gemini: "Gemini CLI",
  grokbuild: "Grok Build",
  opencode: "OpenCode",
  pi: "Pi",
  omp: "Oh My Pi",
  mcode: "MiniMax Code",
  kimi: "Kimi Code",
  dsh: "DeepSeek Harness",
  agy: "Antigravity",
  copilot: "GitHub Copilot CLI",
  workbuddy: "WorkBuddy",
};

export interface PlatformAccount {
  id: string;
  name: string | null;
  email: string | null;
  plan: string | null;
  tags: string[];
  nickname?: string;
  phone?: string;
  pinned?: boolean;
  current?: boolean;
  /** Unix timestamp in milliseconds. */
  updatedAt: number | null;
  quotaError: boolean;
  quotas: PlatformQuota[];
}

export interface PlatformQuota {
  label: string;
  used: number | null;
  limit: number | null;
  balance: number | null;
  remainingPercent: number | null;
  /** Unix timestamp in milliseconds. */
  resetAt: number | null;
  unit: string | null;
  unlimited: boolean;
}

export interface PlatformLogin {
  loginId: string;
  url: string;
  userCode?: string | null;
}

export interface PlatformSwitchResult {
  account: PlatformAccount;
  warning?: string | null;
}

type AccountAction =
  | { operation: "list" }
  | { operation: "import_current" }
  | { operation: "import"; json: string }
  | { operation: "refresh" | "remove" | "switch" | "export"; account_id: string }
  | { operation: "edit"; account_id: string; nickname: string; tags: string[] }
  | { operation: "set_phone"; account_id: string; phone: string }
  | { operation: "set_pinned"; account_id: string; pinned: boolean }
  | { operation: "login_start"; option?: string }
  | { operation: "login_finish"; login_id: string; code?: string }
  | { operation: "login_callback"; login_id: string; code: string }
  | { operation: "login_cancel"; login_id: string };

interface UsageTotals {
  requests: number;
  cost: number;
  unpriced: number;
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  tokens: number;
  cacheHitRate: number | null;
}

export interface UsageRequest {
  id: string;
  provider: string;
  app: string;
  model: string;
  createdAt: number;
  status: number;
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  cost: number | null;
  durationMs: number | null;
  firstTokenMs: number | null;
  tokensPerSecond: number | null;
}

export interface ExtensionUsage {
  summary: UsageTotals;
  timeline: (UsageTotals & { start: number })[];
  providers: (UsageTotals & { key: string })[];
  models: (UsageTotals & { key: string })[];
  requests: UsageRequest[];
  pageSize: number;
  providerOptions: string[];
  providerApplications?: Record<string, string[]>;
  modelOptions: string[];
  statusOptions: number[];
}

export interface UsageFilter {
  start: number;
  end: number;
  buckets: number[];
  page: number;
  app?: string;
  provider?: string;
  model?: string;
  status?: number;
}

export interface UsageSyncResult {
  imported: number;
  deferred: number;
  failed: number;
  completedAt: number;
  applications?: string[];
}

export function listExtensions(): Promise<InstalledExtension[]> {
  return invoke("extension_list");
}

export interface ExtensionImportResult {
  installed: InstalledExtension[];
  failed: { file: string; error: string }[];
}

export function importExtensions(): Promise<ExtensionImportResult> {
  return invoke("extension_import");
}

export function setExtensionEnabled(extensionId: string, enabled: boolean): Promise<void> {
  return invoke("extension_set_enabled", { extensionId, enabled });
}

export function uninstallExtension(extensionId: string): Promise<void> {
  return invoke("extension_uninstall", { extensionId });
}

export function callPlatform<T>(extensionId: string, platform: string, action: AccountAction): Promise<T> {
  return invoke("extension_call", { extensionId, platform, action });
}

export function extensionUsage(extensionId: string, filter: UsageFilter): Promise<ExtensionUsage> {
  const query = { operation: "dashboard", ...filter };
  return invoke("extension_call", { extensionId, action: { operation: "usage", query } });
}

export function syncExtensionUsage(extensionId: string): Promise<UsageSyncResult> {
  const query = { operation: "sync" };
  return invoke("extension_call", { extensionId, action: { operation: "usage", query } });
}

export function listExtensionUsageApps(extensionId: string): Promise<string[]> {
  const query = { operation: "applications" };
  return invoke("extension_call", { extensionId, action: { operation: "usage", query } });
}
