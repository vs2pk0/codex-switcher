<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import { getVersion } from "@tauri-apps/api/app";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import AccountList from "./components/AccountList.vue";
import ApiServicePanel from "./components/ApiServicePanel.vue";
import BadgeStyleModal from "./components/BadgeStyleModal.vue";
import PlanBadge from "./components/PlanBadge.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import UsagePanel from "./components/UsagePanel.vue";
import { defaultBadgeStyles } from "./constants/badgeStyles";
import {
  addCodexAccountWithApiKey,
  cancelCodexOAuthLogin,
  completeCodexOAuthLogin,
  consumeCodexResetCredit,
  deleteCodexAccount,
  deleteCodexSwitcherBackup,
  exportCodexAccounts,
  getCodexSwitcherPaths,
  getCodexSwitcherSettings,
  getCurrentCodexAccount,
  importCodexFromJson,
  importCodexFromLocal,
  listCodexSwitcherBackups,
  listCodexSwitcherSessionBackups,
  listCodexAccounts,
  openExternalUrl,
  refreshCodexQuota,
  resetCodexConfigToml,
  restoreCodexSwitcherBackup,
  restoreCodexSwitcherSessionBackup,
  restartCodexApp,
  startCodexSwitcherBackup,
  startCodexSwitcherSessionBackup,
  startCodexOAuthLogin,
  switchCodexAccount,
  submitCodexOAuthCallbackUrl,
  updateCodexSwitcherSettings,
  updateCodexAccountPhone,
  updateCodexAccountFromJson,
  updateCodexAccountProfile,
  updateCodexApiKeyBoundOAuthAccount,
  updateCodexApiKeyCredentials,
  type CodexExportFormat,
  type CodexSwitcherBackupFile,
  type CodexSwitcherBackupProgressEvent,
  type CodexSwitcherPaths,
  type CodexSwitcherSettings,
} from "./services/codex";
import { bindApiServiceAccounts } from "./services/apiService";
import { getCodexUsageDashboard } from "./services/usage";
import {
  listSessionVisibilityRepairInstances,
  listSessionVisibilityRepairProviders,
  listSessionsAcrossInstances,
  listTrashedSessionsAcrossInstances,
  moveSessionsToTrashAcrossInstances,
  openPathInFileManager,
  repairSessionVisibilityAcrossInstances,
  restoreSessionsFromTrashAcrossInstances,
  type CodexSessionRecord,
  type CodexSessionTokenStats,
  type CodexSessionVisibilityRepairInstanceOption,
  type CodexSessionVisibilityRepairMode,
  type CodexSessionVisibilityRepairProviderOption,
  type CodexSessionVisibilityRepairSummary,
  type CodexTrashedSessionRecord,
} from "./services/session";
import type { CodexAccount } from "./types/codex";

type ActiveView = "accounts" | "sessions" | "usage" | "apiService" | "settings" | "about";

const activeView = ref<ActiveView>("accounts");
const appVersion = ref("0.1.0");
const usagePanelMounted = ref(false);
const accounts = ref<CodexAccount[]>([]);
const currentAccount = ref<CodexAccount | null>(null);
const loading = ref(false);
const switchingId = ref("");
const deletingId = ref("");
const quotaRefreshingId = ref("");
const selectedAccountIds = ref<Set<string>>(new Set());
const draggingAccountId = ref("");
const sortEditorVisible = ref(false);
const sortDraftIds = ref<string[]>([]);
const sortDraftDraggingId = ref("");
const sortDraftOverId = ref("");
const currentPage = ref(1);
let quotaTimer: number | undefined;
let currentAccountQuotaTimer: number | undefined;
let quotaCountdownTimer: number | undefined;
let countdownSettingsPersistTimer: number | undefined;
const nextQuotaRefreshAt = ref(0);
const nextCurrentAccountRefreshAt = ref(0);
const nowMs = ref(Date.now());
const addModalVisible = ref(false);
const badgeStyleVisible = ref(false);
const privacyMasked = ref(false);
const addTab = ref("oauth");
const tokenInput = ref("");
const fileInput = ref<HTMLInputElement | null>(null);
const importing = ref(false);
const savingApiKey = ref(false);
const settingsLoading = ref(false);
const savingSettings = ref(false);
const appPaths = ref<CodexSwitcherPaths | null>(null);
const backupFiles = ref<CodexSwitcherBackupFile[]>([]);
const backupLoading = ref(false);
const sessionBackupFiles = ref<CodexSwitcherBackupFile[]>([]);
const sessionBackupLoading = ref(false);
const backupWorking = ref(false);
const backupProgressVisible = ref(false);
const backupProgress = ref(0);
const backupProgressMessage = ref("");
const backupProgressTitle = ref("正在备份");
const backupProgressStatus = ref<"running" | "completed" | "failed">("running");
const backupButtonText = computed(() =>
  backupWorking.value ? `备份 ${Math.round(backupProgress.value)}%` : "备份",
);
const sessionRestoreVisible = ref(false);
const expandedLayout = ref(false);
let windowResizeTimer: number | undefined;
let viewLoadTimer: number | undefined;
let initialPrewarmTimer: number | undefined;
const settings = reactive<CodexSwitcherSettings>({
  monitorQuota: false,
  quotaRefreshMinutes: 10,
  currentAccountRefreshMinutes: 10,
  quotaNextRefreshAt: 0,
  currentAccountNextRefreshAt: 0,
  sortMode: "created_at",
  sortDirection: "desc",
  customOrder: [],
  pinnedAccountIds: [],
  accountTypeFilter: "all",
  pageSize: 50,
  showQuotaCountdowns: true,
  badgeStyle: "classic",
  badgeStyles: defaultBadgeStyles(),
  maxColumns: 3,
});

const oauthLoginId = ref("");
const oauthUrl = ref("");
const oauthCallbackInput = ref("");
const oauthPreparing = ref(false);
const oauthCompleting = ref(false);
const oauthError = ref("");
const oauthCallbackReceived = ref(false);
let oauthUnlisten: UnlistenFn | null = null;

interface OAuthCallbackEvent {
  loginId: string;
  ok: boolean;
  message: string;
}

const apiKeyForm = reactive({
  apiKey: "",
  apiBaseUrl: "https://api.openai.com/v1",
  apiProviderName: "OpenAI Official",
  apiOfficialUrl: "",
  accountName: "",
  boundOauthAccountId: "",
});

const editVisible = ref(false);
const editingAccount = ref<CodexAccount | null>(null);
const editTab = ref("form");
const editJsonText = ref("");
const editForm = reactive({
  accountName: "",
  apiKey: "",
  apiBaseUrl: "",
  apiProviderName: "",
  apiOfficialUrl: "",
});
const editing = ref(false);

const exportVisible = ref(false);
const exportAccount = ref<CodexAccount | null>(null);
const exportText = ref("");
const exportPreviewVisible = ref(false);
const exportFormat = ref<CodexExportFormat>("cockpit_tools");
const exportingId = ref("");
const batchExportVisible = ref(false);
const batchExportText = ref("");
const batchExportPreviewVisible = ref(false);
const exportFormatOptions: { label: string; value: CodexExportFormat }[] = [
  { label: "Codex Switcher", value: "cockpit_tools" },
  { label: "sub2api 格式", value: "sub2api" },
  { label: "cpa 格式", value: "cpa" },
];

const phoneVisible = ref(false);
const phoneAccount = ref<CodexAccount | null>(null);
const phoneForm = reactive({ phone: "" });
const savingPhone = ref(false);

const bindingVisible = ref(false);
const bindingAccount = ref<CodexAccount | null>(null);
const bindingForm = reactive({ boundOauthAccountId: "" });
const savingBinding = ref(false);

const sessions = ref<CodexSessionRecord[]>([]);
const trashedSessions = ref<CodexTrashedSessionRecord[]>([]);
const sessionStats = ref<CodexSessionTokenStats[]>([]);
const selectedSessionIds = ref<Set<string>>(new Set());
const expandedSessionGroups = ref<Set<string>>(new Set());
const sessionLoading = ref(false);
const sessionRepairing = ref(false);
const repairVisible = ref(false);
const switchRepairVisible = ref(false);
const switchRepairProgress = ref(0);
const switchRepairResult = ref<CodexSessionVisibilityRepairSummary | null>(null);
const switchRepairError = ref("");
const switchRepairTargetName = ref("");
let switchRepairCloseTimer: number | undefined;
const repairMode = ref<CodexSessionVisibilityRepairMode>("quick");
const repairInstanceScope = ref<"target" | "all">("target");
const repairSessionScope = ref<"all" | "selected">("all");
const repairTargetInstanceId = ref("__default__");
const repairInstances = ref<CodexSessionVisibilityRepairInstanceOption[]>([]);
const repairProviders = ref<CodexSessionVisibilityRepairProviderOption[]>([]);
const repairResult = ref<CodexSessionVisibilityRepairSummary | null>(null);
const sessionTrashMode = ref(false);
const sessionSearch = reactive({
  titleQuery: "",
  contentQuery: "",
});

interface SessionGroup {
  key: string;
  projectName: string;
  sessions: CodexSessionRecord[];
  latestUpdatedAt: number;
  approximateTokens: number;
}

const currentId = computed(() => currentAccount.value?.id ?? "");
const quotaSortModes = new Set(["weekly_quota", "hourly_quota", "weekly_reset", "hourly_reset", "subscription"]);
const filteredAccounts = computed(() => {
  const filter = settings.accountTypeFilter || "all";
  return accounts.value.filter((account) => {
    if (filter === "all") return true;
    if (filter === "oauth") return !isApiKeyAccount(account);
    if (filter === "apikey") return isApiKeyAccount(account);
    if (filter === "error") return isAccountAbnormal(account);
    if (filter === "valid") return !isAccountAbnormal(account);
    if (filter === "pro") return normalizePlanKey(account.plan_type) === "pro";
    if (filter === "team") return ["team", "business", "enterprise", "edu", "go"].includes(normalizePlanKey(account.plan_type));
    return normalizePlanKey(account.plan_type) === filter;
  });
});
const sortedAccounts = computed(() => {
  const order = new Map(settings.customOrder.map((id, index) => [id, index]));
  const pinned = new Map((settings.pinnedAccountIds || []).map((id, index) => [id, index]));
  const sortDirection = settings.sortDirection === "asc" ? 1 : -1;
  const sortValue = (account: CodexAccount): number => {
    switch (settings.sortMode) {
      case "weekly_quota":
        return account.quota?.weekly_percentage ?? Number.NEGATIVE_INFINITY;
      case "hourly_quota":
        return account.quota?.hourly_percentage ?? Number.NEGATIVE_INFINITY;
      case "weekly_reset":
        return account.quota?.weekly_reset_time ?? Number.NEGATIVE_INFINITY;
      case "hourly_reset":
        return account.quota?.hourly_reset_time ?? Number.NEGATIVE_INFINITY;
      case "subscription":
        return dateSortValue(accountSubscriptionUntil(account));
      case "custom":
        return order.get(account.id) ?? Number.MAX_SAFE_INTEGER;
      case "created_at":
      default:
        return account.created_at;
    }
  };
  return [...filteredAccounts.value].sort((a, b) => {
    const aPinned = pinned.get(a.id);
    const bPinned = pinned.get(b.id);
    if (aPinned !== undefined || bPinned !== undefined) {
      if (aPinned === undefined) return 1;
      if (bPinned === undefined) return -1;
      return aPinned - bPinned;
    }
    if (settings.sortMode !== "custom") {
      const groupDiff = accountSortGroup(a) - accountSortGroup(b);
      if (groupDiff !== 0) return groupDiff;
    }
    const left = sortValue(a);
    const right = sortValue(b);
    if (left !== right) {
      return settings.sortMode === "custom" ? left - right : (left - right) * sortDirection;
    }
    return b.last_used - a.last_used;
  });
});
const totalPages = computed(() =>
  Math.max(1, Math.ceil(sortedAccounts.value.length / Math.max(1, settings.pageSize || 50))),
);
const pagedAccounts = computed(() => {
  const pageSize = Math.max(1, settings.pageSize || 50);
  const page = Math.min(currentPage.value, totalPages.value);
  const start = (page - 1) * pageSize;
  return sortedAccounts.value.slice(start, start + pageSize);
});
const isCurrentPageSelected = computed(
  () => pagedAccounts.value.length > 0 && pagedAccounts.value.every((account) => selectedAccountIds.value.has(account.id)),
);
const accountTypeOptions = computed(() => {
  const count = (predicate: (account: CodexAccount) => boolean) => accounts.value.filter(predicate).length;
  return [
    { label: `全部 (${accounts.value.length})`, value: "all" },
    { label: `OAuth (${oauthCount.value})`, value: "oauth" },
    { label: `API Key (${apiKeyCount.value})`, value: "apikey" },
    { label: `FREE (${count((account) => !isApiKeyAccount(account) && normalizePlanKey(account.plan_type) === "free")})`, value: "free" },
    { label: `PLUS (${count((account) => !isApiKeyAccount(account) && normalizePlanKey(account.plan_type) === "plus")})`, value: "plus" },
    { label: `PRO (${count((account) => !isApiKeyAccount(account) && normalizePlanKey(account.plan_type) === "pro")})`, value: "pro" },
    {
      label: `TEAM (${count((account) =>
        !isApiKeyAccount(account) && ["team", "business", "enterprise", "edu", "go"].includes(normalizePlanKey(account.plan_type)),
      )})`,
      value: "team",
    },
    { label: `异常 (${count(isAccountAbnormal)})`, value: "error" },
    { label: `有效账号 (${count((account) => !isAccountAbnormal(account))})`, value: "valid" },
  ];
});
const oauthCount = computed(
  () => accounts.value.filter((account) => !isApiKeyAccount(account)).length,
);
const apiKeyCount = computed(
  () => accounts.value.filter((account) => isApiKeyAccount(account)).length,
);
const oauthAccounts = computed(() => accounts.value.filter((account) => !isApiKeyAccount(account)));
const sortDraftAccounts = computed(() => {
  const accountMap = new Map(accounts.value.map((account) => [account.id, account]));
  return sortDraftIds.value
    .map((id) => accountMap.get(id))
    .filter((account): account is CodexAccount => Boolean(account));
});
const selectedSessionIdList = computed(() => [...selectedSessionIds.value]);
const activeSessionIds = computed(() =>
  sessionTrashMode.value ? trashedSessions.value.map((session) => session.id) : sessions.value.map((session) => session.id),
);
const allSessionsSelected = computed(
  () => activeSessionIds.value.length > 0 && activeSessionIds.value.every((id) => selectedSessionIds.value.has(id)),
);
const sessionGroups = computed<SessionGroup[]>(() => {
  const groups = new Map<string, SessionGroup>();
  for (const session of sessions.value) {
    const key = sessionGroupKey(session);
    const group = groups.get(key) ?? {
      key,
      projectName: session.projectName || "未归属项目",
      sessions: [],
      latestUpdatedAt: 0,
      approximateTokens: 0,
    };
    group.sessions.push(session);
    group.latestUpdatedAt = Math.max(group.latestUpdatedAt, session.updatedAt);
    group.approximateTokens += sessionStats.value.find((item) => item.sessionId === session.id)?.approximateTokens ?? 0;
    groups.set(key, group);
  }
  return [...groups.values()]
    .map((group) => ({
      ...group,
      sessions: [...group.sessions].sort((left, right) => right.updatedAt - left.updatedAt),
    }))
    .sort((left, right) => right.latestUpdatedAt - left.latestUpdatedAt);
});
const selectedAccountIdList = computed(() => [...selectedAccountIds.value]);
const effectiveRepairSessionScope = computed(() =>
  repairSessionScope.value === "selected" && selectedSessionIdList.value.length
    ? "selected"
    : "all",
);
const repairTargetInstance = computed(
  () => repairInstances.value.find((item) => item.id === repairTargetInstanceId.value),
);
const editTitle = computed(() => {
  if (!editingAccount.value) return "编辑账号";
  return isApiKeyAccount(editingAccount.value) ? "编辑 API Key" : "编辑 OAuth 账号";
});
const quotaRefreshCountdown = computed(() => {
  if (!settings.monitorQuota || !settings.showQuotaCountdowns || !nextQuotaRefreshAt.value) return "";
  return formatCountdown(nextQuotaRefreshAt.value - nowMs.value);
});
const currentAccountRefreshCountdown = computed(() => {
  if (
    !settings.monitorQuota ||
    !settings.showQuotaCountdowns ||
    !currentAccount.value ||
    !canShowQuota(currentAccount.value) ||
    !nextCurrentAccountRefreshAt.value
  ) {
    return "";
  }
  return formatCountdown(nextCurrentAccountRefreshAt.value - nowMs.value);
});
const showSortDirection = computed(() => quotaSortModes.has(settings.sortMode));

function clampRefreshMinutes(value: unknown, fallback = 10): number {
  const minutes = Number(value);
  if (!Number.isFinite(minutes)) return fallback;
  return Math.max(1, Math.min(1440, Math.round(minutes)));
}

function normalizedNextRefreshAt(value: unknown, minutes: number): number {
  const intervalMs = clampRefreshMinutes(minutes) * 60_000;
  const parsed = Number(value || 0);
  const now = Date.now();
  if (Number.isFinite(parsed) && parsed > now && parsed - now <= intervalMs) {
    return Math.floor(parsed);
  }
  return now + intervalMs;
}

function scheduleCountdownSettingsPersist(): void {
  if (countdownSettingsPersistTimer) {
    window.clearTimeout(countdownSettingsPersistTimer);
  }
  countdownSettingsPersistTimer = window.setTimeout(() => {
    countdownSettingsPersistTimer = undefined;
    void updateCodexSwitcherSettings({
      ...settings,
      badgeStyles: {
        ...defaultBadgeStyles(),
        ...settings.badgeStyles,
      },
      sortDirection: settings.sortDirection === "asc" ? "asc" : "desc",
      pinnedAccountIds: settings.pinnedAccountIds || [],
      accountTypeFilter: settings.accountTypeFilter || "all",
      pageSize: Math.max(1, Number(settings.pageSize || 50)),
      quotaRefreshMinutes: clampRefreshMinutes(settings.quotaRefreshMinutes),
      currentAccountRefreshMinutes: clampRefreshMinutes(settings.currentAccountRefreshMinutes),
      quotaNextRefreshAt: settings.monitorQuota ? Math.floor(settings.quotaNextRefreshAt || 0) : 0,
      currentAccountNextRefreshAt: settings.monitorQuota
        ? Math.floor(settings.currentAccountNextRefreshAt || 0)
        : 0,
      showQuotaCountdowns: settings.showQuotaCountdowns ?? true,
      maxColumns: [3, 4, 5].includes(settings.maxColumns) ? settings.maxColumns : 3,
    }).catch(() => {
      // 倒计时缓存失败不影响主流程，下一次正常保存设置会带上最新时间。
    });
  }, 300);
}

function setQuotaNextRefreshAt(value: number, persist = true): void {
  const next = Math.max(0, Math.floor(value || 0));
  nextQuotaRefreshAt.value = next;
  settings.quotaNextRefreshAt = next;
  if (persist) scheduleCountdownSettingsPersist();
}

function setCurrentAccountNextRefreshAt(value: number, persist = true): void {
  const next = Math.max(0, Math.floor(value || 0));
  nextCurrentAccountRefreshAt.value = next;
  settings.currentAccountNextRefreshAt = next;
  if (persist) scheduleCountdownSettingsPersist();
}

function formatCountdown(remainingMs: number): string {
  const totalSeconds = Math.max(0, Math.ceil(remainingMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

function isApiKeyAccount(account: CodexAccount): boolean {
  return account.auth_mode === "apikey" || Boolean(account.openai_api_key || account.openaiApiKey);
}

function displayName(account: CodexAccount): string {
  return account.account_name || account.email || account.id;
}

function maskDisplayText(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return "";
  if (trimmed.includes("@")) {
    const [name, domain] = trimmed.split("@");
    if (!domain) return `${name.slice(0, 2)}***`;
    if (name.length <= 3) return `${name[0] ?? "*"}***@${domain}`;
    return `${name.slice(0, 2)}***${name.slice(-1)}@${domain}`;
  }
  if (trimmed.length <= 3) return `${trimmed[0]}***`;
  return `${trimmed.slice(0, 2)}***${trimmed.slice(-2)}`;
}

function displayNameForUi(account: CodexAccount): string {
  const name = displayName(account);
  return privacyMasked.value ? maskDisplayText(name) : name;
}

function quotaErrorLabel(account: CodexAccount): string {
  if (account.quota_error?.code === "token_expired") return "Token 失效";
  if (account.quota_error) return "额度异常";
  return "";
}

function boundOAuthName(account: CodexAccount): string {
  const bound = boundOAuthAccount(account);
  return bound ? displayName(bound) : "未绑定";
}

function boundOAuthAccount(account: CodexAccount): CodexAccount | undefined {
  const boundId = account.bound_oauth_account_id;
  if (!boundId) return undefined;
  return accounts.value.find((item) => item.id === boundId);
}

function isBoundApiKeyAccount(account: CodexAccount): boolean {
  return isApiKeyAccount(account) && Boolean(account.bound_oauth_account_id);
}

function sessionGroupKey(session: CodexSessionRecord): string {
  return session.projectName || "未归属项目";
}

function canShowQuota(account: CodexAccount): boolean {
  if (!settings.monitorQuota) return false;
  return !isApiKeyAccount(account);
}

function shouldShowQuota(account: CodexAccount): boolean {
  return canShowQuota(account) && Boolean(account.quota);
}

function shouldShowQuotaError(account: CodexAccount): boolean {
  return canShowQuota(account) && Boolean(account.quota_error);
}

function isAccountAbnormal(account: CodexAccount): boolean {
  if (account.quota_error) return true;
  if (!isApiKeyAccount(account) && tokenExpiryStatus(accountTokenExpiresAt(account)) === "expired") return true;
  return false;
}

function accountSortGroup(account: CodexAccount): number {
  if (!isApiKeyAccount(account) && !isAccountAbnormal(account)) return 0;
  if (isApiKeyAccount(account)) return 1;
  return 2;
}

function scalarText(value: unknown): string | undefined {
  if (typeof value === "string") return value.trim() || undefined;
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  return undefined;
}

function jwtPayload(token: string | undefined): Record<string, unknown> | undefined {
  const payload = token?.split(".")[1];
  if (!payload) return undefined;
  try {
    const normalized = payload.replace(/-/g, "+").replace(/_/g, "/");
    const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
    return JSON.parse(window.atob(padded)) as Record<string, unknown>;
  } catch {
    return undefined;
  }
}

function jwtClaim(token: string | undefined, claim: string): string | undefined {
  return scalarText(jwtPayload(token)?.[claim]);
}

function jwtAuthClaim(token: string | undefined, claim: string): string | undefined {
  const auth = jwtPayload(token)?.["https://api.openai.com/auth"];
  if (!auth || typeof auth !== "object" || Array.isArray(auth)) return undefined;
  return scalarText((auth as Record<string, unknown>)[claim]);
}

function accountStatusSource(account: CodexAccount): CodexAccount {
  if (isBoundApiKeyAccount(account)) return account;
  return account.subscription_active_until ||
    account.access_token_expires_at ||
    jwtAuthClaim(account.tokens.id_token, "chatgpt_subscription_active_until") ||
    jwtClaim(account.tokens.access_token, "exp") ||
    jwtClaim(account.tokens.id_token, "exp")
    ? account
    : boundOAuthAccount(account) ?? account;
}

function accountSubscriptionUntil(account: CodexAccount): string | undefined {
  const source = accountStatusSource(account);
  return (
    source.subscription_active_until ||
    jwtAuthClaim(source.tokens.id_token, "chatgpt_subscription_active_until")
  );
}

function accountTokenExpiresAt(account: CodexAccount): string | undefined {
  const source = accountStatusSource(account);
  return (
    source.access_token_expires_at ||
    jwtClaim(source.tokens.access_token, "exp") ||
    jwtClaim(source.tokens.id_token, "exp")
  );
}

function shortAccountId(account: CodexAccount): string {
  const value = account.id || account.email || displayName(account);
  if (value.length <= 8) return value;
  return `${value.slice(0, 3)}****${value.slice(-3)}`;
}

function planLabel(account: CodexAccount): string {
  if (isApiKeyAccount(account)) return "API_KEY";
  const base = planDisplayName(account.plan_type);
  if (base !== "PRO") return base;
  const authPlan = normalizeAuthFilePlan(account.auth_file_plan_type || account.plan_type);
  return authPlan === "prolite" ? "PRO 5X" : "PRO 20X";
}

function planClass(account: CodexAccount): string {
  const badgeKey = badgeTypeKey(account);
  const styleName =
    settings.badgeStyles?.[badgeKey] ||
    settings.badgeStyle ||
    "classic";
  const style = `badge-${styleName}`;
  if (isApiKeyAccount(account)) return `api ${style}`;
  const key = normalizePlanKey(account.plan_type);
  if (key === "pro") {
    const proClass = normalizeAuthFilePlan(account.auth_file_plan_type || account.plan_type) === "prolite"
      ? "pro-lite"
      : "pro-max";
    return `${proClass} ${style}`;
  }
  return `${key} ${style}`;
}

function badgeTypeKey(account: CodexAccount): string {
  if (isApiKeyAccount(account)) return "api";
  const key = normalizePlanKey(account.plan_type);
  if (key === "pro") {
    return normalizeAuthFilePlan(account.auth_file_plan_type || account.plan_type) === "prolite"
      ? "proLite"
      : "proMax";
  }
  if (key === "plus") return "plus";
  if (["team", "business", "enterprise", "edu", "go"].includes(key)) return "team";
  return "free";
}

function planDisplayName(planType?: string): string {
  const key = normalizePlanKey(planType);
  if (key === "enterprise") return "ENTERPRISE";
  if (key === "business") return "BUSINESS";
  if (key === "team") return "TEAM";
  if (key === "edu") return "EDU";
  if (key === "go") return "GO";
  if (key === "plus") return "PLUS";
  if (key === "pro") return "PRO";
  if (key === "api_key") return "API_KEY";
  if (key === "free") return "FREE";
  return (planType || "FREE").trim().toUpperCase();
}

function normalizePlanKey(planType?: string): string {
  const normalized = (planType || "").trim().toLowerCase();
  if (!normalized) return "free";
  if (normalized.includes("api")) return "api_key";
  if (normalized.includes("enterprise")) return "enterprise";
  if (normalized.includes("business")) return "business";
  if (normalized.includes("team")) return "team";
  if (normalized.includes("edu")) return "edu";
  if (normalized.includes("go")) return "go";
  if (normalized.includes("plus")) return "plus";
  if (normalized.includes("pro")) return "pro";
  if (normalized.includes("free")) return "free";
  return normalized;
}

function normalizeAuthFilePlan(value?: string): "prolite" | "promax" | undefined {
  const normalized = (value || "").trim().toLowerCase().replace(/[_\s]+/g, "-");
  if (["prolite", "pro-lite", "pro-5x", "codex-pro-5x"].includes(normalized)) return "prolite";
  if (["promax", "pro-max", "pro-20x", "codex-pro-20x"].includes(normalized)) return "promax";
  return undefined;
}

function accountLoginLine(account: CodexAccount): string {
  if (isApiKeyAccount(account)) return `API Key: ${maskSecret(account.openai_api_key || account.openaiApiKey)}`;
  return `使用 OAuth 登录 | 用户 ID: ${shortAccountId(account)}`;
}

function apiBaseUrlLine(account: CodexAccount): string {
  const baseUrl = (account.api_base_url || account.apiBaseUrl)?.trim();
  return baseUrl ? `Base URL: ${baseUrl}` : "Base URL: 未设置";
}

function apiOfficialUrl(account: CodexAccount): string {
  return (account.api_official_url || account.apiOfficialUrl)?.trim() ?? "";
}

function maskSecret(value?: string): string {
  const trimmed = value?.trim() ?? "";
  if (!trimmed) return "未保存";
  if (trimmed.length <= 10) return `${trimmed.slice(0, 3)}****`;
  return `${trimmed.slice(0, 6)}****${trimmed.slice(-4)}`;
}

function maskExportJson(value: string): string {
  if (!value.trim()) return "";
  try {
    const parsed = JSON.parse(value);
    return JSON.stringify(maskSensitiveJson(parsed), null, 2);
  } catch {
    return value.replace(
      /(eyJ[\w.-]{16,}|rt_[\w.-]{8,}|sk-[\w.-]{8,}|proxy-[\w.-]{8,})/g,
      (match) => maskSecret(match),
    );
  }
}

function exportJsonSummary(value: string): string {
  if (!value.trim()) return "暂无可导出的 JSON";
  try {
    const parsed = maskSensitiveJson(JSON.parse(value));
    return collapsedJsonPreview(parsed);
  } catch {
    return value.trim() || "暂无可导出的 JSON";
  }
}

function collapsedJsonPreview(value: unknown, depth = 0): string {
  const indent = "  ".repeat(depth);
  const nextIndent = "  ".repeat(depth + 1);
  if (Array.isArray(value)) {
    if (!value.length) return "[]";
    const previewItems = value.slice(0, 3).map((item, index) => {
      const comma = index < Math.min(value.length, 3) - 1 || value.length > 3 ? "," : "";
      return `${nextIndent}${collapsedJsonPreview(item, depth + 1)}${comma}`;
    });
    const rest = value.length > 3 ? [`${nextIndent}/* ... ${value.length - 3} more */`] : [];
    return ["[", ...previewItems, ...rest, `${indent}]`].join("\n");
  }
  if (value && typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>);
    if (!entries.length) return "{}";
    const visibleEntries = entries.slice(0, 8);
    const previewEntries = visibleEntries.map(([key, item], index) => {
      const suffix = collapsedJsonEntryPreview(key, item, depth + 1);
      const comma = index < visibleEntries.length - 1 || entries.length > 8 ? "," : "";
      return `${nextIndent}"${key}": ${suffix}${comma}`;
    });
    const rest = entries.length > 8 ? [`${nextIndent}/* ... ${entries.length - 8} more */`] : [];
    return ["{", ...previewEntries, ...rest, `${indent}}`].join("\n");
  }
  return JSON.stringify(value);
}

function collapsedJsonEntryPreview(key: string, value: unknown, depth: number): string {
  if (Array.isArray(value)) {
    if (key === "accounts") return collapsedJsonPreview(value, depth);
    return `[${value.length} items]`;
  }
  if (value && typeof value === "object") {
    const objectValue = value as Record<string, unknown>;
    const previewKeys = ["email", "account_name", "name", "auth_mode", "api_provider_name"];
    const previewEntries = previewKeys
      .filter((previewKey) => objectValue[previewKey] !== undefined)
      .map((previewKey) => `"${previewKey}": ${JSON.stringify(objectValue[previewKey])}`);
    return previewEntries.length ? `{ ${previewEntries.join(", ")} }` : "{...}";
  }
  return JSON.stringify(value) ?? "null";
}

function maskSensitiveJson(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(maskSensitiveJson);
  if (value && typeof value === "object") {
    const next: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(value as Record<string, unknown>)) {
      const lowerKey = key.toLowerCase();
      if (
        typeof item === "string" &&
        (lowerKey.includes("token") ||
          lowerKey.includes("key") ||
          lowerKey.includes("secret") ||
          lowerKey.includes("authorization"))
      ) {
        next[key] = maskSecret(item);
      } else {
        next[key] = maskSensitiveJson(item);
      }
    }
    return next;
  }
  return value;
}

function formatTime(timestamp: number): string {
  if (!Number.isFinite(timestamp) || timestamp <= 0) return "--";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(timestamp * 1000));
}

function parseFlexibleDate(value?: string): Date | null {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  if (/^\d+$/.test(trimmed)) {
    const raw = Number(trimmed);
    if (!Number.isFinite(raw)) return null;
    return new Date(raw > 10_000_000_000 ? raw : raw * 1000);
  }
  const parsed = new Date(trimmed);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}

function formatDateTime(value?: string): string {
  const date = parseFlexibleDate(value);
  if (!date) return "--";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function expiryDaysLabel(value?: string): string {
  const date = parseFlexibleDate(value);
  if (!date) return "未知";
  const diffDays = Math.ceil((date.getTime() - Date.now()) / 86_400_000);
  if (diffDays < 0) return `已过期 ${Math.abs(diffDays)}天`;
  if (diffDays === 0) return "今天到期";
  return `${diffDays}天`;
}

function tokenExpiryStatus(value?: string): "normal" | "expired" {
  const date = parseFlexibleDate(value);
  if (!date) return "normal";
  return date.getTime() <= Date.now() ? "expired" : "normal";
}

function dateSortValue(value?: string): number {
  const date = parseFlexibleDate(value);
  return date ? date.getTime() : Number.NEGATIVE_INFINITY;
}

function quotaWindowLabel(minutes?: number, fallback = "5h"): string {
  if (!minutes || !Number.isFinite(minutes)) return fallback;
  if (minutes % (60 * 24 * 7) === 0) return `${minutes / (60 * 24 * 7)} Week`;
  if (minutes % (60 * 24) === 0) return `${minutes / (60 * 24)} Day`;
  if (minutes % 60 === 0) return `${minutes / 60}h`;
  return `${minutes}m`;
}

function quotaResetLabel(timestamp?: number): string {
  if (!timestamp) return "";
  const diff = timestamp - Math.floor(Date.now() / 1000);
  if (diff <= 0) return "已重置";
  const minutes = Math.floor(diff / 60);
  const days = Math.floor(minutes / 1440);
  const hours = Math.floor((minutes % 1440) / 60);
  const mins = minutes % 60;
  const rel = days > 0 ? `${days}d ${hours}h ${mins}m` : `${hours}h ${mins}m`;
  return `${rel} (${formatTime(timestamp)})`;
}

function quotaColor(percentage: number): string {
  if (percentage < 30) return "#ef4444";
  if (percentage < 60) return "#f59e0b";
  return "#22c55e";
}

function resetCreditCount(account: CodexAccount): number {
  const count = account.quota?.reset_credits_available;
  return Number.isFinite(count) ? Math.max(0, Number(count)) : 0;
}

function canUseResetCredit(account: CodexAccount): boolean {
  return shouldShowQuota(account) && resetCreditCount(account) > 0;
}

function isFreePlanAccount(account: CodexAccount): boolean {
  return !isApiKeyAccount(account) && normalizePlanKey(account.plan_type) === "free";
}

function errorText(error: unknown): string {
  return String(error instanceof Error ? error.message : error).replace(/^Error:\s*/, "");
}

async function loadAccounts(): Promise<void> {
  loading.value = true;
  try {
    const [nextAccounts, nextCurrent] = await Promise.all([
      listCodexAccounts(),
      getCurrentCodexAccount(),
    ]);
    accounts.value = nextAccounts;
    currentAccount.value = nextCurrent;
  } catch (error) {
    Message.error(`加载账号失败：${errorText(error)}`);
  } finally {
    loading.value = false;
  }
}

async function loadSettings(): Promise<void> {
  settingsLoading.value = true;
  try {
    const [nextSettings, nextPaths, nextBackups] = await Promise.all([
      getCodexSwitcherSettings(),
      getCodexSwitcherPaths(),
      listCodexSwitcherBackups(),
    ]);
    Object.assign(settings, {
      ...nextSettings,
      badgeStyles: {
        ...defaultBadgeStyles(),
        ...(nextSettings.badgeStyles || {}),
      },
      sortDirection: nextSettings.sortDirection === "asc" ? "asc" : "desc",
      pinnedAccountIds: nextSettings.pinnedAccountIds || [],
      accountTypeFilter: nextSettings.accountTypeFilter || "all",
      pageSize: Math.max(1, Number(nextSettings.pageSize || 50)),
      quotaRefreshMinutes: clampRefreshMinutes(nextSettings.quotaRefreshMinutes),
      currentAccountRefreshMinutes: clampRefreshMinutes(nextSettings.currentAccountRefreshMinutes),
      quotaNextRefreshAt: Number(nextSettings.quotaNextRefreshAt || 0),
      currentAccountNextRefreshAt: Number(nextSettings.currentAccountNextRefreshAt || 0),
      showQuotaCountdowns: nextSettings.showQuotaCountdowns ?? true,
      maxColumns: [3, 4, 5].includes(nextSettings.maxColumns) ? nextSettings.maxColumns : 3,
    });
    appPaths.value = nextPaths;
    backupFiles.value = nextBackups;
    resetQuotaTimer();
  } catch (error) {
    Message.error(`加载设置失败：${errorText(error)}`);
  } finally {
    settingsLoading.value = false;
  }
}

async function loadBackups(options: { silent?: boolean } = {}): Promise<void> {
  backupLoading.value = true;
  try {
    backupFiles.value = await listCodexSwitcherBackups();
  } catch (error) {
    if (!options.silent) Message.error(`加载备份列表失败：${errorText(error)}`);
  } finally {
    backupLoading.value = false;
  }
}

async function loadSessionBackups(options: { silent?: boolean } = {}): Promise<void> {
  sessionBackupLoading.value = true;
  try {
    sessionBackupFiles.value = await listCodexSwitcherSessionBackups();
  } catch (error) {
    if (!options.silent) Message.error(`加载会话备份列表失败：${errorText(error)}`);
  } finally {
    sessionBackupLoading.value = false;
  }
}

async function saveSettings(): Promise<void> {
  savingSettings.value = true;
  try {
    const saved = await updateCodexSwitcherSettings({
      ...settings,
      badgeStyles: {
        ...defaultBadgeStyles(),
        ...settings.badgeStyles,
      },
      sortDirection: settings.sortDirection === "asc" ? "asc" : "desc",
      pinnedAccountIds: settings.pinnedAccountIds || [],
      accountTypeFilter: settings.accountTypeFilter || "all",
      pageSize: Math.max(1, Number(settings.pageSize || 50)),
      quotaRefreshMinutes: clampRefreshMinutes(settings.quotaRefreshMinutes),
      currentAccountRefreshMinutes: clampRefreshMinutes(settings.currentAccountRefreshMinutes),
      quotaNextRefreshAt: settings.monitorQuota ? Math.floor(settings.quotaNextRefreshAt || 0) : 0,
      currentAccountNextRefreshAt: settings.monitorQuota
        ? Math.floor(settings.currentAccountNextRefreshAt || 0)
        : 0,
      showQuotaCountdowns: settings.showQuotaCountdowns ?? true,
      maxColumns: [3, 4, 5].includes(settings.maxColumns) ? settings.maxColumns : 3,
    });
    Object.assign(settings, saved);
    resetQuotaTimer();
    Message.success("设置已保存");
  } catch (error) {
    Message.error(`保存设置失败：${errorText(error)}`);
  } finally {
    savingSettings.value = false;
  }
}

function resetCurrentAccountQuotaTimer(forceNew = false): void {
  if (currentAccountQuotaTimer) {
    window.clearTimeout(currentAccountQuotaTimer);
    currentAccountQuotaTimer = undefined;
  }
  const storedNextAt = settings.currentAccountNextRefreshAt;
  setCurrentAccountNextRefreshAt(0, false);
  if (!settings.monitorQuota || !currentAccount.value || !canShowQuota(currentAccount.value)) return;
  const minutes = clampRefreshMinutes(settings.currentAccountRefreshMinutes);
  const nextAt = forceNew
    ? Date.now() + minutes * 60_000
    : normalizedNextRefreshAt(storedNextAt, minutes);
  setCurrentAccountNextRefreshAt(nextAt);
  currentAccountQuotaTimer = window.setTimeout(() => {
    void handleRefreshCurrentQuota(false).finally(() => {
      if (settings.monitorQuota && currentAccount.value) {
        resetCurrentAccountQuotaTimer(true);
      }
    });
  }, Math.max(1_000, nextAt - Date.now()));
}

function resetQuotaTimer(forceNew = false): void {
  if (quotaTimer) {
    window.clearTimeout(quotaTimer);
    quotaTimer = undefined;
  }
  const storedNextAt = settings.quotaNextRefreshAt;
  setQuotaNextRefreshAt(0, false);
  if (!settings.monitorQuota) {
    resetCurrentAccountQuotaTimer();
    return;
  }
  const minutes = clampRefreshMinutes(settings.quotaRefreshMinutes);
  const nextAt = forceNew
    ? Date.now() + minutes * 60_000
    : normalizedNextRefreshAt(storedNextAt, minutes);
  setQuotaNextRefreshAt(nextAt);
  quotaTimer = window.setTimeout(() => {
    void handleRefreshAllQuotas(false).finally(() => {
      if (settings.monitorQuota) {
        resetQuotaTimer(true);
      }
    });
  }, Math.max(1_000, nextAt - Date.now()));
  resetCurrentAccountQuotaTimer();
}

async function handleRefreshCurrentQuota(showMessage = true): Promise<void> {
  if (!settings.monitorQuota || !currentAccount.value || !canShowQuota(currentAccount.value)) {
    return;
  }
  const accountId = currentAccount.value.id;
  quotaRefreshingId.value = accountId;
  try {
    const updated = await refreshCodexQuota(accountId);
    currentAccount.value = updated;
    await loadAccounts();
    setCurrentAccountNextRefreshAt(
      Date.now() + clampRefreshMinutes(settings.currentAccountRefreshMinutes) * 60_000,
    );
    if (showMessage) Message.success("当前账号额度已刷新");
  } catch (error) {
    await loadAccounts();
    if (showMessage) Message.warning(`当前账号额度刷新失败：${errorText(error)}`);
  } finally {
    quotaRefreshingId.value = "";
  }
}

async function handleRefreshQuota(account: CodexAccount): Promise<void> {
  quotaRefreshingId.value = account.id;
  try {
    await refreshCodexQuota(account.id);
    await loadAccounts();
    Message.success("额度已刷新");
  } catch (error) {
    await loadAccounts();
    Message.warning(`额度刷新失败：${errorText(error)}`);
  } finally {
    quotaRefreshingId.value = "";
  }
}

function confirmResetCredit(account: CodexAccount): void {
  const count = resetCreditCount(account);
  if (count <= 0) {
    Message.warning("当前账号没有可用的重置次数");
    return;
  }
  Modal.warning({
    title: "确认重置额度",
    content: `将消耗 1 次重置次数来重置 ${displayName(account)} 的当前 Codex 额度窗口。当前可用 ${count} 次，是否继续？`,
    okText: "确认重置",
    cancelText: "取消",
    hideCancel: false,
    async onOk() {
      quotaRefreshingId.value = account.id;
      try {
        await consumeCodexResetCredit(account.id);
        await loadAccounts();
        Message.success("额度已重置");
      } catch (error) {
        await loadAccounts();
        Message.error(`重置额度失败：${errorText(error)}`);
      } finally {
        quotaRefreshingId.value = "";
      }
    },
  });
}

async function handleRefreshAllQuotas(showMessage = true): Promise<void> {
  if (!settings.monitorQuota) return;
  quotaRefreshingId.value = "__all__";
  try {
    const candidates = pagedAccounts.value.filter(canShowQuota);
    for (const account of candidates) {
      quotaRefreshingId.value = account.id;
      await refreshCodexQuota(account.id);
    }
    await loadAccounts();
    if (settings.monitorQuota) {
      setQuotaNextRefreshAt(Date.now() + clampRefreshMinutes(settings.quotaRefreshMinutes) * 60_000);
    }
    if (showMessage) Message.success(`已刷新当前页 ${candidates.length} 个账号额度`);
  } catch (error) {
    if (showMessage) Message.warning(`批量刷新额度失败：${errorText(error)}`);
  } finally {
    quotaRefreshingId.value = "";
  }
}

async function runSwitchSessionRepair(account: CodexAccount): Promise<void> {
  clearSwitchRepairCloseTimer();
  switchRepairTargetName.value = displayName(account);
  switchRepairVisible.value = true;
  switchRepairProgress.value = 12;
  switchRepairResult.value = null;
  switchRepairError.value = "";
  try {
    switchRepairProgress.value = 48;
    const summary = await repairSessionVisibilityAcrossInstances({
      mode: "quick",
      targetProvider: null,
      targetInstanceId: "__default__",
      repairInstanceIds: ["__default__"],
    });
    switchRepairResult.value = summary;
    switchRepairProgress.value = 100;
    scheduleSwitchRepairAutoClose();
  } catch (error) {
    switchRepairError.value = errorText(error);
    switchRepairProgress.value = 100;
    throw error;
  }
}

async function handleSwitch(account: CodexAccount): Promise<void> {
  switchingId.value = account.id;
  try {
    currentAccount.value = await switchCodexAccount(account.id);
    try {
      await runSwitchSessionRepair(account);
    } catch (repairError) {
      Message.warning(`账号已切换，会话修复失败：${errorText(repairError)}`);
    }
    let restartMessage = "已尝试重启 Codex";
    try {
      restartMessage = await restartCodexApp();
    } catch (restartError) {
      Message.warning(`账号已切换，但启动 Codex 失败：${errorText(restartError)}`);
      await loadAccounts();
      return;
    }
    await loadAccounts();
    Message.success(`已切换到 ${displayName(account)}，${restartMessage}`);
  } catch (error) {
    Message.error(`切换失败：${errorText(error)}`);
  } finally {
    switchingId.value = "";
  }
}

function confirmDelete(account: CodexAccount): void {
  Modal.warning({
    title: "删除账号",
    content: `确认删除 ${displayName(account)}？此操作只删除本工具保存的账号记录，不会删除 Codex 程序本身。`,
    okText: "删除",
    cancelText: "取消",
    hideCancel: false,
    onOk: () => handleDelete(account),
  });
}

async function handleDelete(account: CodexAccount): Promise<void> {
  deletingId.value = account.id;
  try {
    await deleteCodexAccount(account.id);
    await loadAccounts();
    Message.success(`已删除 ${displayName(account)}`);
  } catch (error) {
    Message.error(`删除失败：${errorText(error)}`);
  } finally {
    deletingId.value = "";
  }
}

function toggleAccount(accountId: string): void {
  const next = new Set(selectedAccountIds.value);
  if (next.has(accountId)) {
    next.delete(accountId);
  } else {
    next.add(accountId);
  }
  selectedAccountIds.value = next;
}

function toggleAllAccounts(checked: boolean): void {
  const next = new Set(selectedAccountIds.value);
  for (const account of pagedAccounts.value) {
    if (checked) next.add(account.id);
    else next.delete(account.id);
  }
  selectedAccountIds.value = next;
}

async function openBatchExport(): Promise<void> {
  const ids = selectedAccountIdList.value.length
    ? selectedAccountIdList.value
    : sortedAccounts.value.map((account) => account.id);
  if (!ids.length) {
    Message.warning("没有可导出的账号");
    return;
  }
  try {
    batchExportText.value = await exportCodexAccounts(ids, exportFormat.value);
    batchExportPreviewVisible.value = false;
    batchExportVisible.value = true;
  } catch (error) {
    Message.error(`批量导出失败：${errorText(error)}`);
  }
}

function confirmBindSelectedToApiService(): void {
  const selected = selectedAccountIdList.value
    .map((id) => accounts.value.find((account) => account.id === id))
    .filter((account): account is CodexAccount => Boolean(account))
    .filter((account) => !isApiKeyAccount(account));
  if (!selectedAccountIdList.value.length) {
    Message.warning("请先勾选要绑定到 API 服务的 OAuth 账号");
    return;
  }
  if (!selected.length) {
    Message.warning("API Key 账号不需要绑定到 API 服务，请选择 OAuth 账号");
    return;
  }
  Modal.warning({
    title: "绑定到 API 服务",
    content: `将 ${selected.length} 个 OAuth 账号转换为 CPA 格式，并写入 API 服务认证目录。是否继续？`,
    okText: "确认绑定",
    cancelText: "取消",
    hideCancel: false,
    async onOk() {
      try {
        const summary = await bindApiServiceAccounts(selected.map((account) => account.id));
        Message.success(`已绑定 ${summary.count} 个账号到 API 服务`);
      } catch (error) {
        Message.error(`绑定失败：${errorText(error)}`);
      }
    },
  });
}

async function refreshBatchExportText(): Promise<void> {
  const ids = selectedAccountIdList.value.length
    ? selectedAccountIdList.value
    : sortedAccounts.value.map((account) => account.id);
  batchExportText.value = await exportCodexAccounts(ids, exportFormat.value);
  batchExportPreviewVisible.value = false;
}

async function copyBatchExportText(): Promise<void> {
  try {
    await navigator.clipboard.writeText(batchExportText.value);
    Message.success("已复制 JSON");
  } catch {
    Message.error("复制失败，请手动选择内容复制");
  }
}

function downloadBatchExportText(): void {
  const blob = new Blob([batchExportText.value], { type: "application/json;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  const suffix = exportFormat.value === "cockpit_tools" ? "" : `_${exportFormat.value}`;
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  link.download = `codex-switcher-batch${suffix}_${timestamp}.json`;
  link.click();
  URL.revokeObjectURL(url);
}

function handleDragStart(account: CodexAccount): void {
  if (settings.sortMode !== "custom") return;
  draggingAccountId.value = account.id;
}

function handleDragEnd(): void {
  draggingAccountId.value = "";
}

function normalizedCustomOrder(): string[] {
  const allIds = accounts.value.map((account) => account.id);
  const validIds = new Set(allIds);
  const orderedIds = new Set<string>();
  const next: string[] = [];
  for (const id of settings.customOrder || []) {
    if (!validIds.has(id) || orderedIds.has(id)) continue;
    orderedIds.add(id);
    next.push(id);
  }
  for (const account of sortedAccounts.value) {
    if (orderedIds.has(account.id)) continue;
    orderedIds.add(account.id);
    next.push(account.id);
  }
  for (const id of allIds) {
    if (orderedIds.has(id)) continue;
    orderedIds.add(id);
    next.push(id);
  }
  return next;
}

async function handleDropAccount(target: CodexAccount): Promise<void> {
  if (settings.sortMode !== "custom" || !draggingAccountId.value || draggingAccountId.value === target.id) {
    draggingAccountId.value = "";
    return;
  }
  const currentOrder = normalizedCustomOrder();
  const sourceId = draggingAccountId.value;
  const withoutSource = currentOrder.filter((id) => id !== sourceId);
  const targetIndex = withoutSource.indexOf(target.id);
  withoutSource.splice(targetIndex < 0 ? withoutSource.length : targetIndex, 0, sourceId);
  settings.customOrder = withoutSource;
  draggingAccountId.value = "";
  await saveSettings();
}

function openSortEditor(): void {
  if (settings.sortMode !== "custom") {
    Message.info("先选择自定义顺序再编辑排序");
    return;
  }
  sortDraftIds.value = normalizedCustomOrder();
  sortDraftDraggingId.value = "";
  sortDraftOverId.value = "";
  sortEditorVisible.value = true;
}

function closeSortEditor(): void {
  sortEditorVisible.value = false;
  sortDraftDraggingId.value = "";
  sortDraftOverId.value = "";
  window.removeEventListener("pointerup", handleSortDraftPointerEnd);
  window.removeEventListener("pointercancel", handleSortDraftPointerEnd);
}

function moveSortDraft(sourceId: string, targetId: string): void {
  if (!sourceId || !targetId || sourceId === targetId) return;
  const current = [...sortDraftIds.value];
  const sourceIndex = current.indexOf(sourceId);
  const targetIndex = current.indexOf(targetId);
  if (sourceIndex < 0 || targetIndex < 0) return;
  current.splice(sourceIndex, 1);
  current.splice(targetIndex, 0, sourceId);
  sortDraftIds.value = current;
}

function handleSortDraftPointerStart(event: PointerEvent, account: CodexAccount): void {
  if (event.button !== 0) return;
  sortDraftDraggingId.value = account.id;
  sortDraftOverId.value = account.id;
  window.addEventListener("pointerup", handleSortDraftPointerEnd, { once: true });
  window.addEventListener("pointercancel", handleSortDraftPointerEnd, { once: true });
}

function handleSortDraftPointerEnter(account: CodexAccount): void {
  const sourceId = sortDraftDraggingId.value;
  if (!sourceId || sourceId === account.id) return;
  sortDraftOverId.value = account.id;
  moveSortDraft(sourceId, account.id);
}

function handleSortDraftPointerEnd(): void {
  sortDraftDraggingId.value = "";
  sortDraftOverId.value = "";
  window.removeEventListener("pointerup", handleSortDraftPointerEnd);
  window.removeEventListener("pointercancel", handleSortDraftPointerEnd);
}

function moveSortDraftByStep(account: CodexAccount, offset: number): void {
  const current = [...sortDraftIds.value];
  const index = current.indexOf(account.id);
  const nextIndex = index + offset;
  if (index < 0 || nextIndex < 0 || nextIndex >= current.length) return;
  current.splice(index, 1);
  current.splice(nextIndex, 0, account.id);
  sortDraftIds.value = current;
}

async function saveSortEditor(): Promise<void> {
  const draft = sortDraftIds.value.filter((id, index, ids) => ids.indexOf(id) === index);
  const draftSet = new Set(draft);
  const remaining = accounts.value.map((account) => account.id).filter((id) => !draftSet.has(id));
  settings.customOrder = [...draft, ...remaining];
  currentPage.value = 1;
  closeSortEditor();
  await saveSettings();
}

async function toggleAccountPin(account: CodexAccount): Promise<void> {
  const next = [...(settings.pinnedAccountIds || [])];
  const index = next.indexOf(account.id);
  if (index >= 0) next.splice(index, 1);
  else next.unshift(account.id);
  settings.pinnedAccountIds = next;
  await saveSettings();
}

async function handleTokenImport(): Promise<void> {
  const value = tokenInput.value.trim();
  if (!value) {
    Message.error("请输入 Token 或 JSON");
    return;
  }
  importing.value = true;
  try {
    const imported = await importCodexFromJson(value);
    tokenInput.value = "";
    addModalVisible.value = false;
    await loadAccounts();
    Message.success(`成功导入 ${imported.length} 个账号`);
  } catch (error) {
    Message.error(`导入失败：${errorText(error)}`);
  } finally {
    importing.value = false;
  }
}

async function handleLocalImport(): Promise<void> {
  importing.value = true;
  try {
    const imported = await importCodexFromLocal();
    addModalVisible.value = false;
    await loadAccounts();
    Message.success(`已从本机 Codex 导入 ${imported.length} 个账号`);
  } catch (error) {
    Message.error(`本地导入失败：${errorText(error)}`);
  } finally {
    importing.value = false;
  }
}

function openFileImport(): void {
  fileInput.value?.click();
}

async function handleFileImport(event: Event): Promise<void> {
  const target = event.target as HTMLInputElement;
  const files = [...(target.files ?? [])];
  target.value = "";
  if (!files.length) return;
  importing.value = true;
  try {
    let count = 0;
    for (const file of files) {
      const content = await file.text();
      const imported = await importCodexFromJson(content);
      count += imported.length;
    }
    addModalVisible.value = false;
    await loadAccounts();
    Message.success(`已从 ${files.length} 个文件导入 ${count} 个账号`);
  } catch (error) {
    Message.error(`文件导入失败：${errorText(error)}`);
  } finally {
    importing.value = false;
  }
}

async function handleApiKeyAdd(): Promise<void> {
  if (!apiKeyForm.apiKey.trim()) {
    Message.error("请输入 API Key");
    return;
  }
  savingApiKey.value = true;
  try {
    const account = await addCodexAccountWithApiKey({
      apiKey: apiKeyForm.apiKey.trim(),
      apiBaseUrl: apiKeyForm.apiBaseUrl.trim(),
      apiProviderName: apiKeyForm.apiProviderName.trim(),
      apiOfficialUrl: apiKeyForm.apiOfficialUrl.trim(),
      accountName: apiKeyForm.accountName.trim(),
      boundOauthAccountId: apiKeyForm.boundOauthAccountId,
    });
    apiKeyForm.apiKey = "";
    apiKeyForm.accountName = "";
    apiKeyForm.apiOfficialUrl = "";
    apiKeyForm.boundOauthAccountId = "";
    addModalVisible.value = false;
    await loadAccounts();
    Message.success(`已添加 ${displayName(account)}`);
  } catch (error) {
    Message.error(`添加失败：${errorText(error)}`);
  } finally {
    savingApiKey.value = false;
  }
}

async function prepareOAuthLogin(): Promise<void> {
  oauthPreparing.value = true;
  oauthError.value = "";
  try {
    if (oauthLoginId.value) {
      await cancelCodexOAuthLogin(oauthLoginId.value).catch(() => undefined);
    }
    const response = await startCodexOAuthLogin();
    oauthLoginId.value = response.loginId;
    oauthUrl.value = response.authUrl;
    oauthCallbackInput.value = "";
    oauthCallbackReceived.value = false;
  } catch (error) {
    oauthError.value = errorText(error);
  } finally {
    oauthPreparing.value = false;
  }
}

async function copyOAuthUrl(): Promise<void> {
  try {
    await navigator.clipboard.writeText(oauthUrl.value);
    Message.success("已复制授权链接");
  } catch {
    Message.error("复制失败，请手动选择链接复制");
  }
}

function openOAuthUrl(): void {
  if (!oauthUrl.value) return;
  void openExternalUrl(oauthUrl.value).catch((error) => {
    Message.error(`打开浏览器失败：${errorText(error)}`);
  });
}

async function startOrOpenOAuthUrl(): Promise<void> {
  if (!oauthUrl.value) {
    await prepareOAuthLogin();
  }
  if (oauthUrl.value) {
    openOAuthUrl();
  }
}

function openOfficialUrl(url: string): void {
  if (!url.trim()) return;
  void openExternalUrl(url.trim()).catch((error) => {
    Message.error(`打开官网失败：${errorText(error)}`);
  });
}

function openGithubProfile(): void {
  void openExternalUrl("https://github.com/vs2pk0").catch((error) => {
    Message.error(`打开 GitHub 失败：${errorText(error)}`);
  });
}

async function completeOAuthLoginFlow(loginId: string): Promise<void> {
  oauthCompleting.value = true;
  oauthError.value = "";
  try {
    const account = await completeCodexOAuthLogin(loginId);
    addModalVisible.value = false;
    oauthLoginId.value = "";
    oauthUrl.value = "";
    oauthCallbackInput.value = "";
    oauthCallbackReceived.value = false;
    await loadAccounts();
    Message.success(`已添加 ${displayName(account)}`);
  } catch (error) {
    oauthError.value = errorText(error);
    Message.error(`OAuth 授权失败：${oauthError.value}`);
  } finally {
    oauthCompleting.value = false;
  }
}

async function handleOAuthCallbackSubmit(): Promise<void> {
  if (!oauthLoginId.value) {
    Message.error("请先生成授权链接");
    return;
  }
  if (!oauthCallbackInput.value.trim()) {
    await completeOAuthLoginFlow(oauthLoginId.value);
    return;
  }
  try {
    await submitCodexOAuthCallbackUrl({
      loginId: oauthLoginId.value,
      callbackUrl: oauthCallbackInput.value.trim(),
    });
    oauthCallbackReceived.value = true;
    await completeOAuthLoginFlow(oauthLoginId.value);
  } catch (error) {
    oauthError.value = errorText(error);
    Message.error(`OAuth 授权失败：${oauthError.value}`);
  }
}

async function openEdit(account: CodexAccount): Promise<void> {
  editingAccount.value = account;
  editTab.value = "form";
  editJsonText.value = JSON.stringify(account, null, 2);
  editForm.accountName = account.account_name ?? "";
  editForm.apiKey = account.openai_api_key ?? account.openaiApiKey ?? "";
  editForm.apiBaseUrl = account.api_base_url ?? account.apiBaseUrl ?? "https://api.openai.com/v1";
  editForm.apiProviderName = account.api_provider_name ?? account.apiProviderName ?? "OpenAI Official";
  editForm.apiOfficialUrl = account.api_official_url ?? account.apiOfficialUrl ?? "";
  editVisible.value = true;
  try {
    editJsonText.value = await exportCodexAccounts([account.id], "cockpit_tools");
  } catch (error) {
    Message.warning(`读取完整 JSON 失败，已使用当前缓存：${errorText(error)}`);
  }
}

function hasPreviewJsonPlaceholders(value: string): boolean {
  return (
    /\/\*\s*\.\.\./.test(value) ||
    /\{\s*\.\.\.\s*\}/.test(value) ||
    /\[\d+\s+items\]/.test(value) ||
    /"\s*[^"]*\*{3,}[^"]*"\s*/.test(value)
  );
}

async function handleEditSave(): Promise<void> {
  if (!editingAccount.value) return;
  if (editTab.value === "form" && isApiKeyAccount(editingAccount.value) && !editForm.apiKey.trim()) {
    Message.error("请输入 API Key");
    return;
  }
  if (editTab.value === "json" && !editJsonText.value.trim()) {
    Message.error("JSON 不能为空");
    return;
  }
  if (editTab.value === "json" && hasPreviewJsonPlaceholders(editJsonText.value)) {
    Message.error("当前内容像是预览 JSON，包含省略或隐藏字段，请粘贴完整 JSON 后保存");
    return;
  }
  editing.value = true;
  try {
    const account = editingAccount.value;
    let updated: CodexAccount;
    if (editTab.value === "json") {
      updated = await updateCodexAccountFromJson({
        accountId: account.id,
        jsonContent: editJsonText.value.trim(),
      });
    } else {
      updated = await updateCodexAccountProfile({
        accountId: account.id,
        accountName: editForm.accountName.trim(),
      });
      if (isApiKeyAccount(account)) {
        updated = await updateCodexApiKeyCredentials({
          accountId: account.id,
          apiKey: editForm.apiKey.trim(),
          apiBaseUrl: editForm.apiBaseUrl.trim(),
          apiProviderName: editForm.apiProviderName.trim(),
          apiOfficialUrl: editForm.apiOfficialUrl.trim(),
        });
      }
    }
    editVisible.value = false;
    await loadAccounts();
    Message.success(`已更新 ${displayName(updated)}`);
  } catch (error) {
    Message.error(`保存失败：${errorText(error)}`);
  } finally {
    editing.value = false;
  }
}

async function openExport(account: CodexAccount): Promise<void> {
  exportingId.value = account.id;
  try {
    exportAccount.value = account;
    exportFormat.value = "cockpit_tools";
    exportText.value = await exportCodexAccounts([account.id], exportFormat.value);
    exportPreviewVisible.value = false;
    exportVisible.value = true;
  } catch (error) {
    Message.error(`导出失败：${errorText(error)}`);
  } finally {
    exportingId.value = "";
  }
}

async function refreshExportText(): Promise<void> {
  if (!exportAccount.value) return;
  try {
    exportText.value = await exportCodexAccounts([exportAccount.value.id], exportFormat.value);
    exportPreviewVisible.value = false;
  } catch (error) {
    Message.error(`导出失败：${errorText(error)}`);
  }
}

async function copyExportText(): Promise<void> {
  try {
    await navigator.clipboard.writeText(exportText.value);
    Message.success("已复制 JSON");
  } catch {
    Message.error("复制失败，请手动选择内容复制");
  }
}

function downloadExportText(): void {
  const name = exportAccount.value ? displayName(exportAccount.value).replace(/[^\w.-]+/g, "_") : "codex-account";
  const blob = new Blob([exportText.value], { type: "application/json;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  const suffix = exportFormat.value === "cockpit_tools" ? "" : `_${exportFormat.value}`;
  link.download = `${name}${suffix}.json`;
  link.click();
  URL.revokeObjectURL(url);
}

function openPhone(account: CodexAccount): void {
  phoneAccount.value = account;
  phoneForm.phone = account.bound_phone ?? "";
  phoneVisible.value = true;
}

async function handlePhoneSave(): Promise<void> {
  if (!phoneAccount.value) return;
  savingPhone.value = true;
  try {
    const updated = await updateCodexAccountPhone({
      accountId: phoneAccount.value.id,
      phone: phoneForm.phone.trim(),
    });
    phoneVisible.value = false;
    await loadAccounts();
    Message.success(updated.bound_phone ? "已绑定手机号" : "已清空手机号");
  } catch (error) {
    Message.error(`保存失败：${errorText(error)}`);
  } finally {
    savingPhone.value = false;
  }
}

function openBinding(account: CodexAccount): void {
  bindingAccount.value = account;
  bindingForm.boundOauthAccountId = account.bound_oauth_account_id ?? "";
  bindingVisible.value = true;
}

async function handleBindingSave(): Promise<void> {
  if (!bindingAccount.value) return;
  savingBinding.value = true;
  try {
    const updated = await updateCodexApiKeyBoundOAuthAccount({
      accountId: bindingAccount.value.id,
      boundOauthAccountId: bindingForm.boundOauthAccountId || null,
    });
    if (updated.id === currentId.value) {
      await repairSessionVisibilityAcrossInstances();
    }
    bindingVisible.value = false;
    await loadAccounts();
    Message.success(updated.bound_oauth_account_id ? "已绑定 OAuth 账号" : "已解绑 OAuth 账号");
  } catch (error) {
    Message.error(`绑定失败：${errorText(error)}`);
  } finally {
    savingBinding.value = false;
  }
}

async function loadSessions(options: { silent?: boolean } = {}): Promise<void> {
  sessionLoading.value = true;
  try {
    if (sessionTrashMode.value) {
      trashedSessions.value = await listTrashedSessionsAcrossInstances();
      sessions.value = [];
      sessionStats.value = [];
    } else {
      sessions.value = await listSessionsAcrossInstances({
        titleQuery: sessionSearch.titleQuery,
        contentQuery: sessionSearch.contentQuery,
      });
      trashedSessions.value = [];
      sessionStats.value = sessions.value.map((session) => ({
        sessionId: session.id,
        approximateTokens: Math.ceil((session.charCount || 0) / 4),
        charCount: session.charCount || 0,
      }));
      const firstGroup = sessions.value[0] ? sessionGroupKey(sessions.value[0]) : "";
      expandedSessionGroups.value = firstGroup ? new Set([firstGroup]) : new Set();
    }
    selectedSessionIds.value = new Set();
  } catch (error) {
    if (!options.silent) Message.error(`加载会话失败：${errorText(error)}`);
  } finally {
    sessionLoading.value = false;
  }
}

function toggleSession(sessionId: string): void {
  const next = new Set(selectedSessionIds.value);
  if (next.has(sessionId)) {
    next.delete(sessionId);
  } else {
    next.add(sessionId);
  }
  selectedSessionIds.value = next;
}

function toggleAllSessions(): void {
  selectedSessionIds.value = allSessionsSelected.value
    ? new Set()
    : new Set(activeSessionIds.value);
}

function toggleSessionGroupExpanded(groupKey: string): void {
  const next = new Set(expandedSessionGroups.value);
  if (next.has(groupKey)) {
    next.delete(groupKey);
  } else {
    next.add(groupKey);
  }
  expandedSessionGroups.value = next;
}

function isSessionGroupSelected(group: SessionGroup): boolean {
  return group.sessions.length > 0 && group.sessions.every((session) => selectedSessionIds.value.has(session.id));
}

function toggleSessionGroupSelection(group: SessionGroup): void {
  const next = new Set(selectedSessionIds.value);
  const selected = isSessionGroupSelected(group);
  for (const session of group.sessions) {
    if (selected) next.delete(session.id);
    else next.add(session.id);
  }
  selectedSessionIds.value = next;
}

function sessionApproxTokens(sessionId: string): string {
  const stats = sessionStats.value.find((item) => item.sessionId === sessionId);
  return stats
    ? `${new Intl.NumberFormat("en-US").format(stats.approximateTokens)} tokens`
    : "-- tokens";
}

function clearSwitchRepairCloseTimer(): void {
  if (!switchRepairCloseTimer) return;
  window.clearTimeout(switchRepairCloseTimer);
  switchRepairCloseTimer = undefined;
}

function closeSwitchRepairModal(): void {
  clearSwitchRepairCloseTimer();
  switchRepairVisible.value = false;
}

function scheduleSwitchRepairAutoClose(): void {
  clearSwitchRepairCloseTimer();
  switchRepairCloseTimer = window.setTimeout(() => {
    switchRepairVisible.value = false;
    switchRepairCloseTimer = undefined;
  }, 3000);
}

async function openSessionFolder(path: string): Promise<void> {
  try {
    await openPathInFileManager(path);
  } catch (error) {
    Message.error(`打开文件夹失败：${errorText(error)}`);
  }
}

async function handleRepairSessions(): Promise<void> {
  repairResult.value = null;
  repairMode.value = "quick";
  repairSessionScope.value = selectedSessionIdList.value.length ? "selected" : "all";
  repairInstanceScope.value = "target";
  repairVisible.value = true;
  try {
    const [instances, providers] = await Promise.all([
      listSessionVisibilityRepairInstances(),
      listSessionVisibilityRepairProviders(),
    ]);
    repairInstances.value = instances.instances;
    repairProviders.value = providers.providers;
    repairTargetInstanceId.value =
      instances.defaultInstanceId || instances.instances[0]?.id || "__default__";
  } catch (error) {
    Message.warning(`读取修复选项失败：${errorText(error)}`);
  }
}

async function runRepairSessions(): Promise<void> {
  sessionRepairing.value = true;
  try {
    const target = repairTargetInstance.value;
    const summary = await repairSessionVisibilityAcrossInstances({
      mode: repairMode.value,
      targetProvider: target?.currentProvider ?? repairProviders.value.find((item) => item.isDefault)?.id,
      targetInstanceId: repairTargetInstanceId.value,
      repairInstanceIds:
        repairInstanceScope.value === "target" ? [repairTargetInstanceId.value] : null,
      sessionIds:
        effectiveRepairSessionScope.value === "selected" ? selectedSessionIdList.value : null,
    });
    repairResult.value = summary;
    Message.success(summary.message);
    await loadSessions();
  } catch (error) {
    Message.error(`会话修复失败：${errorText(error)}`);
  } finally {
    sessionRepairing.value = false;
  }
}

function confirmResetConfig(): void {
  Modal.warning({
    title: "重置 config.toml",
    content: "确认删除本机 Codex 目录下的 config.toml？删除后 Codex 会按默认配置重新生成或使用默认设置。",
    okText: "删除",
    cancelText: "取消",
    hideCancel: false,
    onOk: async () => {
      try {
        const deleted = await resetCodexConfigToml();
        Message.success(deleted ? "已删除 config.toml" : "config.toml 不存在，无需重置");
      } catch (error) {
        Message.error(`重置失败：${errorText(error)}`);
      }
    },
  });
}

async function handleTrashSessions(): Promise<void> {
  const ids = selectedSessionIdList.value;
  if (!ids.length) {
    Message.warning("请先选择会话");
    return;
  }
  const summary = await moveSessionsToTrashAcrossInstances(ids);
  Message.success(`已移动 ${summary.moved} 个会话到回收站`);
  await loadSessions();
}

async function handleRestoreSessions(): Promise<void> {
  const ids = selectedSessionIdList.value;
  if (!ids.length) {
    Message.warning("请先选择回收站会话");
    return;
  }
  const summary = await restoreSessionsFromTrashAcrossInstances(ids);
  Message.success(`已恢复 ${summary.restored} 个会话`);
  await loadSessions();
}

function createBackupTaskId(): string {
  return `backup-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

async function handleExportBackup(
  successPrefix = "已生成 ZIP 备份",
  title = "正在备份数据",
  startBackup: (taskId: string) => Promise<string> = startCodexSwitcherBackup,
  reloadBackups: () => Promise<void> = loadBackups,
): Promise<void> {
  if (backupWorking.value) {
    Message.info("已有备份任务正在运行");
    return;
  }
  backupWorking.value = true;
  backupProgressVisible.value = true;
  backupProgress.value = 0;
  backupProgressMessage.value = "正在启动备份任务...";
  backupProgressTitle.value = title;
  backupProgressStatus.value = "running";
  const taskId = createBackupTaskId();
  const unlistenRef: { current: UnlistenFn | null } = { current: null };
  let backupTimeout: number | undefined;
  let backupCompleted = false;
  try {
    let resolveBackup!: (backup: CodexSwitcherBackupFile) => void;
    let rejectBackup!: (error: Error) => void;
    const backupPromise = new Promise<CodexSwitcherBackupFile>((resolve, reject) => {
      resolveBackup = resolve;
      rejectBackup = reject;
    });
    backupTimeout = window.setTimeout(() => {
      rejectBackup(new Error("备份超时，请稍后重试或检查会话目录是否过大"));
    }, 10 * 60 * 1000);
    unlistenRef.current = await listen<CodexSwitcherBackupProgressEvent>(
      "codex-switcher-backup-progress",
      (event) => {
        const payload = event.payload;
        if (!payload || payload.taskId !== taskId) return;
        backupProgress.value = Math.max(0, Math.min(100, Number(payload.progress) || 0));
        backupProgressMessage.value = payload.message || "正在备份...";
        backupProgressStatus.value = payload.status;
        if (payload.status === "completed" && payload.backupFile) {
          resolveBackup(payload.backupFile);
        } else if (payload.status === "completed") {
          rejectBackup(new Error("备份完成但未返回备份文件信息"));
        }
        if (payload.status === "failed") {
          rejectBackup(new Error(payload.message || "备份失败"));
        }
      },
    );
    await startBackup(taskId);
    const backup = await backupPromise;
    await reloadBackups();
    backupCompleted = true;
    Message.success(`${successPrefix}：${backup.name}`);
  } catch (error) {
    backupProgressStatus.value = "failed";
    backupProgress.value = 100;
    backupProgressMessage.value = errorText(error);
    Message.error(`导出备份失败：${errorText(error)}`);
  } finally {
    if (backupTimeout) window.clearTimeout(backupTimeout);
    unlistenRef.current?.();
    backupWorking.value = false;
    if (backupCompleted) {
      window.setTimeout(() => {
        if (!backupWorking.value) backupProgressVisible.value = false;
      }, 1200);
    }
  }
}

async function handleExportSessionBackup(): Promise<void> {
  await handleExportBackup(
    "已备份会话数据",
    "正在备份会话数据",
    startCodexSwitcherSessionBackup,
    loadSessionBackups,
  );
}

async function runRestoreTask(
  title: string,
  startMessage: string,
  restore: () => Promise<string>,
  reload: () => Promise<void>,
): Promise<void> {
  if (backupWorking.value) {
    Message.info("已有备份或恢复任务正在运行");
    return;
  }
  backupWorking.value = true;
  backupProgressVisible.value = true;
  backupProgressTitle.value = title;
  backupProgressStatus.value = "running";
  backupProgress.value = 8;
  backupProgressMessage.value = startMessage;
  try {
    window.setTimeout(() => {
      if (backupWorking.value && backupProgressStatus.value === "running") {
        backupProgress.value = 42;
        backupProgressMessage.value = "正在写入本机数据...";
      }
    }, 200);
    const message = await restore();
    backupProgress.value = 82;
    backupProgressMessage.value = "正在刷新界面数据...";
    await reload();
    backupProgress.value = 100;
    backupProgressStatus.value = "completed";
    backupProgressMessage.value = message;
    Message.success(message);
    window.setTimeout(() => {
      if (!backupWorking.value) backupProgressVisible.value = false;
    }, 1200);
  } catch (error) {
    backupProgressStatus.value = "failed";
    backupProgress.value = 100;
    backupProgressMessage.value = errorText(error);
    Message.error(`恢复失败：${errorText(error)}`);
  } finally {
    backupWorking.value = false;
  }
}

function handleRestoreBackup(backup: CodexSwitcherBackupFile): void {
  Modal.warning({
    title: "恢复完整备份",
    content: `确认使用 ${backup.name} 恢复账号、设置与所有 Codex 会话记录？`,
    okText: "开始恢复",
    cancelText: "取消",
    hideCancel: false,
    async onOk() {
      await runRestoreTask(
        "正在恢复完整备份",
        "正在准备恢复账号、设置与会话数据...",
        async () => {
          const imported = await restoreCodexSwitcherBackup(backup.path);
          return `已恢复备份，导入 ${imported.length} 个账号`;
        },
        async () => {
          await Promise.all([loadAccounts(), loadSettings(), loadSessions()]);
        },
      );
    },
  });
}

function handleRestoreSessionBackup(backup: CodexSwitcherBackupFile): void {
  Modal.warning({
    title: "只恢复会话数据",
    content: `确认从 ${backup.name} 只恢复 Codex 会话、会话回收站与会话索引？账号和设置不会变更。`,
    okText: "开始恢复",
    cancelText: "取消",
    hideCancel: false,
    async onOk() {
      sessionRestoreVisible.value = false;
      await runRestoreTask(
        "正在恢复会话数据",
        "正在准备恢复 Codex 会话数据...",
        async () => {
          await restoreCodexSwitcherSessionBackup(backup.path);
          return "已恢复会话数据";
        },
        async () => {
          await Promise.all([loadSessions(), loadSessionBackups()]);
        },
      );
    },
  });
}

async function openSessionRestoreModal(): Promise<void> {
  if (!sessionBackupFiles.value.length) {
    await loadSessionBackups();
  }
  sessionRestoreVisible.value = true;
}

async function handleDeleteBackup(backup: CodexSwitcherBackupFile): Promise<void> {
  backupWorking.value = true;
  try {
    await deleteCodexSwitcherBackup(backup.path);
    await loadBackups();
    Message.success("备份已删除");
  } catch (error) {
    Message.error(`删除备份失败：${errorText(error)}`);
  } finally {
    backupWorking.value = false;
  }
}

function openAddModal(tab: "oauth" | "token" | "apikey" = "oauth"): void {
  addTab.value = tab;
  addModalVisible.value = true;
  if (tab === "oauth" && !oauthUrl.value) {
    void prepareOAuthLogin();
  }
}

function handleAddTabChange(key: string | number): void {
  if (key === "oauth" && !oauthUrl.value) {
    void prepareOAuthLogin();
  }
}

function switchView(view: ActiveView): void {
  activeView.value = view;
  if (view === "usage") {
    usagePanelMounted.value = true;
  }
  if (viewLoadTimer) {
    window.clearTimeout(viewLoadTimer);
    viewLoadTimer = undefined;
  }
  void nextTick(() => {
    viewLoadTimer = window.setTimeout(() => {
      if (view !== activeView.value) return;
      if (view === "sessions" && !sessions.value.length && !trashedSessions.value.length) {
        void loadSessions();
      }
      if (view === "settings" && !appPaths.value) {
        void loadSettings();
      }
    }, 0);
  });
}

function todayUsageRange(): { startDate: number; endDate: number } {
  const now = new Date();
  const start = new Date(now);
  start.setHours(0, 0, 0, 0);
  const end = new Date(now);
  end.setHours(now.getHours() + 1, 0, 0, 0);
  return {
    startDate: Math.floor(start.getTime() / 1000),
    endDate: Math.floor(end.getTime() / 1000),
  };
}

async function prewarmUsageDashboard(): Promise<void> {
  try {
    const range = todayUsageRange();
    await getCodexUsageDashboard({
      ...range,
      page: 1,
      pageSize: 1,
      refresh: false,
    });
  } catch {
    // 预热失败不打扰首屏，用户进入统计页时仍会正常加载并提示。
  }
}

function scheduleInitialPrewarm(): void {
  if (initialPrewarmTimer) window.clearTimeout(initialPrewarmTimer);
  initialPrewarmTimer = window.setTimeout(() => {
    if (!sessions.value.length && !trashedSessions.value.length) {
      void loadSessions({ silent: true });
    }
    if (!sessionBackupFiles.value.length) {
      void loadSessionBackups({ silent: true });
    }
    void prewarmUsageDashboard();
    usagePanelMounted.value = true;
  }, 900);
}

async function syncExpandedLayout(): Promise<void> {
  try {
    const currentWindow = getCurrentWindow();
    const [maximized, fullscreen] = await Promise.all([
      currentWindow.isMaximized(),
      currentWindow.isFullscreen(),
    ]);
    expandedLayout.value = maximized || fullscreen;
  } catch {
    expandedLayout.value =
      window.innerWidth >= Math.max(1600, window.screen.availWidth - 80);
  }
}

function handleWindowResize(): void {
  if (windowResizeTimer) window.clearTimeout(windowResizeTimer);
  windowResizeTimer = window.setTimeout(() => {
    void syncExpandedLayout();
  }, 80);
}

watch([sortedAccounts, () => settings.pageSize], () => {
  if (currentPage.value > totalPages.value) currentPage.value = totalPages.value;
  if (currentPage.value < 1) currentPage.value = 1;
  const visible = new Set(sortedAccounts.value.map((account) => account.id));
  selectedAccountIds.value = new Set([...selectedAccountIds.value].filter((id) => visible.has(id)));
});

watch(switchRepairVisible, (visible) => {
  if (!visible) clearSwitchRepairCloseTimer();
});

watch(
  () => currentAccount.value?.id,
  () => {
    resetCurrentAccountQuotaTimer();
  },
);

onMounted(() => {
  quotaCountdownTimer = window.setInterval(() => {
    nowMs.value = Date.now();
  }, 1000);
  void getVersion()
    .then((version) => {
      if (version) appVersion.value = version;
    })
    .catch(() => undefined);
  void syncExpandedLayout();
  window.addEventListener("resize", handleWindowResize);
  void loadAccounts();
  void loadSettings();
  scheduleInitialPrewarm();
  void listen<OAuthCallbackEvent>("codex-oauth-callback-received", async (event) => {
    const payload = event.payload;
    if (!payload?.loginId || payload.loginId !== oauthLoginId.value) return;
    if (!payload.ok) {
      oauthError.value = payload.message;
      Message.error(`OAuth 回调失败：${payload.message}`);
      return;
    }
    oauthCallbackReceived.value = true;
    Message.info("已收到 OAuth 回调，正在保存账号");
    await completeOAuthLoginFlow(payload.loginId);
  }).then((unlisten) => {
    oauthUnlisten = unlisten;
  });
});

onUnmounted(() => {
  oauthUnlisten?.();
  window.removeEventListener("resize", handleWindowResize);
  if (windowResizeTimer) window.clearTimeout(windowResizeTimer);
  if (viewLoadTimer) window.clearTimeout(viewLoadTimer);
  if (initialPrewarmTimer) window.clearTimeout(initialPrewarmTimer);
  window.removeEventListener("pointerup", handleSortDraftPointerEnd);
  window.removeEventListener("pointercancel", handleSortDraftPointerEnd);
  clearSwitchRepairCloseTimer();
  if (quotaTimer) window.clearTimeout(quotaTimer);
  if (currentAccountQuotaTimer) window.clearTimeout(currentAccountQuotaTimer);
  if (quotaCountdownTimer) window.clearInterval(quotaCountdownTimer);
  if (countdownSettingsPersistTimer) window.clearTimeout(countdownSettingsPersistTimer);
});
</script>

<template>
  <main class="app-shell">
    <header class="topbar">
      <div class="brand">
        <h1>Codex Switcher</h1>
        <p>管理 OAuth 与 API Key 登录态，并写回本机 Codex 配置。</p>
      </div>
    </header>

    <section v-if="activeView === 'accounts'" class="status-line">
      <a-tag color="arcoblue">全部 {{ accounts.length }}</a-tag>
      <a-tag color="green">OAuth {{ oauthCount }}</a-tag>
      <a-tag color="orange">API Key {{ apiKeyCount }}</a-tag>
      <span v-if="currentAccount">当前：{{ displayNameForUi(currentAccount) }}</span>
      <a-tag v-if="currentAccount && quotaErrorLabel(currentAccount)" color="red">
        {{ quotaErrorLabel(currentAccount) }}
      </a-tag>
    </section>

    <section class="command-bar">
      <div class="view-tabs">
        <a-button :type="activeView === 'accounts' ? 'primary' : 'secondary'" @click="switchView('accounts')">
          账号总览
        </a-button>
        <a-button :type="activeView === 'sessions' ? 'primary' : 'secondary'" @click="switchView('sessions')">
          <template #icon><icon-folder /></template>
          会话管理
        </a-button>
        <a-button :type="activeView === 'usage' ? 'primary' : 'secondary'" @click="switchView('usage')">
          <template #icon><icon-bar-chart /></template>
          使用统计
        </a-button>
        <a-button :type="activeView === 'apiService' ? 'primary' : 'secondary'" @click="switchView('apiService')">
          <template #icon><icon-code /></template>
          API 服务
        </a-button>
        <a-button :type="activeView === 'settings' ? 'primary' : 'secondary'" @click="switchView('settings')">
          <template #icon><icon-settings /></template>
          设置
        </a-button>
        <a-button :type="activeView === 'about' ? 'primary' : 'secondary'" @click="switchView('about')">
          <template #icon><icon-info-circle /></template>
          关于
        </a-button>
      </div>
      <div v-if="activeView === 'accounts'" class="command-actions">
        <a-button @click="privacyMasked = !privacyMasked">
          <template #icon>
            <icon-eye-invisible v-if="privacyMasked" />
            <icon-eye v-else />
          </template>
          {{ privacyMasked ? "已隐藏" : "隐私" }}
        </a-button>
        <a-button @click="badgeStyleVisible = true">
          <template #icon><icon-palette /></template>
          徽章样式
        </a-button>
        <a-button type="primary" @click="openAddModal('oauth')">
          <template #icon><icon-plus /></template>
          添加账号
        </a-button>
      </div>
    </section>

    <section v-if="activeView === 'accounts'" class="account-ops">
      <div class="account-ops-left">
        <a-checkbox
          :model-value="isCurrentPageSelected"
          @change="(checked) => toggleAllAccounts(Boolean(checked))"
        >
          全选
        </a-checkbox>
        <a-select
          v-model="settings.accountTypeFilter"
          class="filter-select"
          popup-container="body"
          :scrollbar="false"
          :trigger-props="{ contentClass: 'account-filter-dropdown' }"
          @change="() => { currentPage = 1; saveSettings(); }"
        >
          <a-option
            v-for="option in accountTypeOptions"
            :key="option.value"
            :value="option.value"
          >
            {{ option.label }}
          </a-option>
        </a-select>
        <a-select v-model="settings.sortMode" class="sort-select" @change="saveSettings">
          <a-option value="created_at">按创建时间</a-option>
          <a-option value="weekly_quota">按周配额</a-option>
          <a-option value="hourly_quota">按5小时配额</a-option>
          <a-option value="weekly_reset">按周配额重置时间</a-option>
          <a-option value="hourly_reset">按5小时配额重置时间</a-option>
          <a-option value="subscription">按订阅有效期</a-option>
          <a-option value="custom">自定义顺序</a-option>
        </a-select>
        <a-button v-if="settings.sortMode === 'custom'" @click="openSortEditor">
          <template #icon><icon-list /></template>
          编辑排序
        </a-button>
        <a-radio-group
          v-if="showSortDirection"
          v-model="settings.sortDirection"
          type="button"
          size="small"
          @change="saveSettings"
        >
          <a-radio value="desc">倒序</a-radio>
          <a-radio value="asc">正序</a-radio>
        </a-radio-group>
        <a-select
          v-model="settings.pageSize"
          class="page-size-select"
          @change="() => { currentPage = 1; saveSettings(); }"
        >
          <a-option :value="20">每页 20</a-option>
          <a-option :value="50">每页 50</a-option>
          <a-option :value="100">每页 100</a-option>
          <a-option :value="200">每页 200</a-option>
        </a-select>
        <a-button @click="confirmBindSelectedToApiService">
          <template #icon><icon-link /></template>
          绑定到 API 服务
        </a-button>
        <a-button @click="openBatchExport">
          <template #icon><icon-download /></template>
          批量导出
        </a-button>
        <a-button @click="openAddModal('token')">
          <template #icon><icon-import /></template>
          批量导入
        </a-button>
      </div>
      <div
        v-if="currentAccountRefreshCountdown || quotaRefreshCountdown"
        class="quota-countdown-group"
      >
        <span v-if="currentAccountRefreshCountdown" class="quota-countdown primary">
          当前账号 {{ currentAccountRefreshCountdown }}
        </span>
        <span v-if="quotaRefreshCountdown" class="quota-countdown">
          当前页 {{ quotaRefreshCountdown }}
        </span>
      </div>
    </section>

    <AccountList
      v-if="activeView === 'accounts'"
      :accounts="pagedAccounts"
      :current-id="currentId"
      :selected-account-ids="selectedAccountIds"
      :settings="settings"
      :expanded-layout="expandedLayout"
      :loading="loading"
      :switching-id="switchingId"
      :deleting-id="deletingId"
      :exporting-id="exportingId"
      :quota-refreshing-id="quotaRefreshingId"
      :privacy-masked="privacyMasked"
      @toggle-account="toggleAccount"
      @toggle-pin="toggleAccountPin"
      @drag-start="handleDragStart"
      @drag-end="handleDragEnd"
      @drop-account="handleDropAccount"
      @open-phone="openPhone"
      @reset-credit="confirmResetCredit"
      @open-binding="openBinding"
      @open-official-url="openOfficialUrl"
      @open-edit="openEdit"
      @switch-account="handleSwitch"
      @refresh-quota="handleRefreshQuota"
      @open-export="openExport"
      @confirm-delete="confirmDelete"
      @open-add="openAddModal"
    />

    <div v-if="activeView === 'accounts' && sortedAccounts.length > settings.pageSize" class="pagination-bar">
      <a-pagination
        v-model:current="currentPage"
        :total="sortedAccounts.length"
        :page-size="settings.pageSize"
        show-total
        show-jumper
      />
    </div>

    <a-modal
      v-model:visible="sortEditorVisible"
      title="编辑账号顺序"
      width="820px"
      :footer="false"
      @cancel="closeSortEditor"
    >
      <div class="sort-editor">
        <div class="sort-editor-hint">
          <span>拖动列表项调整顺序，保存后会写入自定义顺序。</span>
          <b>{{ sortDraftAccounts.length }} 个账号</b>
        </div>
        <div class="sort-editor-list">
          <article
            v-for="(account, index) in sortDraftAccounts"
            :key="account.id"
            class="sort-editor-row"
            :class="{
              dragging: sortDraftDraggingId === account.id,
              over: sortDraftOverId === account.id,
            }"
            @pointerenter="handleSortDraftPointerEnter(account)"
          >
            <button
              class="sort-editor-grip"
              type="button"
              title="按住拖动排序"
              @pointerdown.prevent="handleSortDraftPointerStart($event, account)"
            >
              <icon-list />
            </button>
            <span class="sort-editor-index">{{ index + 1 }}</span>
            <div class="sort-editor-main">
              <strong>{{ displayNameForUi(account) }}</strong>
              <span>{{ isApiKeyAccount(account) ? "API Key" : "OAuth" }} · {{ account.email || account.id }}</span>
            </div>
            <PlanBadge :label="planLabel(account)" :badge-class="planClass(account)" />
            <a-tag v-if="account.id === currentId" color="arcoblue">当前</a-tag>
            <div class="sort-editor-actions">
              <a-button size="mini" :disabled="index === 0" @click="moveSortDraftByStep(account, -1)">
                <template #icon><icon-up /></template>
              </a-button>
              <a-button size="mini" :disabled="index === sortDraftAccounts.length - 1" @click="moveSortDraftByStep(account, 1)">
                <template #icon><icon-down /></template>
              </a-button>
            </div>
          </article>
        </div>
        <div class="sort-editor-footer">
          <a-button @click="closeSortEditor">取消</a-button>
          <a-button type="primary" :loading="savingSettings" @click="saveSortEditor">保存排序</a-button>
        </div>
      </div>
    </a-modal>

    <BadgeStyleModal
      v-model:visible="badgeStyleVisible"
      :settings="settings"
      :saving="savingSettings"
      @save="saveSettings"
    />

    <section v-if="activeView === 'sessions'" class="session-panel">
      <div class="session-toolbar">
        <div class="session-search">
          <a-input
            v-model="sessionSearch.titleQuery"
            allow-clear
            placeholder="搜索会话标题"
            @press-enter="() => loadSessions()"
          />
          <a-input
            v-model="sessionSearch.contentQuery"
            allow-clear
            placeholder="搜索会话内容"
            @press-enter="() => loadSessions()"
          />
        </div>
        <div class="session-actions">
          <a-button
            :disabled="!activeSessionIds.length"
            @click="toggleAllSessions"
          >
            {{ allSessionsSelected ? "取消全选" : (sessionTrashMode ? "全选回收站" : "全选") }}
          </a-button>
          <a-button
            :type="sessionTrashMode ? 'secondary' : 'primary'"
            @click="sessionTrashMode = false; loadSessions()"
          >
            会话列表
          </a-button>
          <a-button
            :type="sessionTrashMode ? 'primary' : 'secondary'"
            @click="sessionTrashMode = true; loadSessions()"
          >
            回收站
          </a-button>
          <a-button :loading="sessionLoading" @click="() => loadSessions()">
            <template #icon><icon-refresh /></template>
            刷新
          </a-button>
          <a-button :loading="backupWorking" @click="handleExportSessionBackup">
            <template #icon><icon-download /></template>
            {{ backupButtonText }}
          </a-button>
          <a-button :loading="sessionBackupLoading" :disabled="backupWorking" @click="openSessionRestoreModal">
            <template #icon><icon-import /></template>
            恢复会话
          </a-button>
          <a-button :loading="sessionRepairing" type="primary" @click="handleRepairSessions">
            <template #icon><icon-tool /></template>
            修复可见性
          </a-button>
          <a-button
            v-if="!sessionTrashMode"
            status="danger"
            :disabled="!selectedSessionIdList.length"
            @click="handleTrashSessions"
          >
            <template #icon><icon-delete /></template>
            移入回收站
          </a-button>
          <a-button
            v-else
            type="primary"
            :disabled="!selectedSessionIdList.length"
            @click="handleRestoreSessions"
          >
            <template #icon><icon-undo /></template>
            恢复
          </a-button>
        </div>
      </div>

      <a-spin :loading="sessionLoading" dot>
        <div v-if="!sessionTrashMode" class="session-list">
          <section v-for="group in sessionGroups" :key="group.key" class="session-group">
            <div class="session-group-row">
              <a-button
                class="session-expand-button"
                size="mini"
                shape="circle"
                @click="toggleSessionGroupExpanded(group.key)"
              >
                <template #icon>
                  <icon-down v-if="expandedSessionGroups.has(group.key)" />
                  <icon-right v-else />
                </template>
              </a-button>
              <a-checkbox
                :model-value="isSessionGroupSelected(group)"
                @change="toggleSessionGroupSelection(group)"
              />
              <icon-folder class="session-group-icon" />
              <button
                class="session-group-title"
                type="button"
                :title="group.projectName"
                @click="toggleSessionGroupExpanded(group.key)"
              >
                {{ group.projectName }}
              </button>
              <span class="session-group-meta">{{ group.sessions.length }} 条会话</span>
              <span class="token-count">{{ new Intl.NumberFormat("en-US").format(group.approximateTokens) }} tokens</span>
              <span class="session-group-time">{{ formatTime(group.latestUpdatedAt) }}</span>
            </div>
            <div v-if="expandedSessionGroups.has(group.key)" class="session-group-children">
              <article v-for="session in group.sessions" :key="session.id" class="session-child-row">
                <a-checkbox
                  :model-value="selectedSessionIds.has(session.id)"
                  @change="toggleSession(session.id)"
                />
                <div class="session-main session-main-name-only">
                  <strong class="session-name-only" :title="session.title">
                    {{ session.title || "未命名会话" }}
                  </strong>
                </div>
                <div class="session-stat">
                  <span class="token-count">{{ sessionApproxTokens(session.id) }}</span>
                  <span>{{ formatTime(session.updatedAt) }}</span>
                  <a-button size="small" @click="openSessionFolder(session.path)">
                    <template #icon><icon-folder /></template>
                    打开文件夹
                  </a-button>
                </div>
              </article>
            </div>
          </section>
          <div v-if="!sessions.length" class="session-empty-state">
            <div class="session-empty-icon">
              <icon-message />
            </div>
            <div class="session-empty-copy">
              <strong>{{ sessionSearch.titleQuery || sessionSearch.contentQuery ? "没有匹配的会话" : "还没有可显示的会话" }}</strong>
              <span>
                {{ sessionSearch.titleQuery || sessionSearch.contentQuery
                  ? "换个关键词试试，或清空搜索后重新刷新。"
                  : "可以先刷新本机会话；如果是切号后看不到旧会话，使用修复可见性重新挂回列表。"
                }}
              </span>
            </div>
            <div class="session-empty-actions">
              <a-button type="primary" :loading="sessionLoading" @click="() => loadSessions()">
                <template #icon><icon-refresh /></template>
                刷新会话
              </a-button>
              <a-button :loading="sessionBackupLoading" :disabled="backupWorking" @click="openSessionRestoreModal">
                <template #icon><icon-import /></template>
                从备份恢复
              </a-button>
              <a-button :loading="sessionRepairing" @click="handleRepairSessions">
                <template #icon><icon-tool /></template>
                修复可见性
              </a-button>
            </div>
          </div>
        </div>

        <div v-else class="session-list">
          <article v-for="session in trashedSessions" :key="session.id" class="session-row">
            <a-checkbox
              :model-value="selectedSessionIds.has(session.id)"
              @change="toggleSession(session.id)"
            />
            <div class="session-main">
              <strong :title="session.title">{{ session.title }}</strong>
              <span :title="session.originalPath">{{ session.originalPath }}</span>
            </div>
            <div class="session-stat">
              <span>已删除</span>
              <span>{{ formatTime(session.deletedAt) }}</span>
              <a-button size="small" @click="openSessionFolder(session.originalPath)">
                <template #icon><icon-folder /></template>
                打开文件夹
              </a-button>
            </div>
          </article>
          <div v-if="!trashedSessions.length" class="session-empty-state compact">
            <div class="session-empty-icon">
              <icon-delete />
            </div>
            <div class="session-empty-copy">
              <strong>回收站为空</strong>
              <span>被移入回收站的会话会显示在这里，恢复后会回到原来的会话路径。</span>
            </div>
          </div>
        </div>
      </a-spin>
    </section>

    <UsagePanel
      v-if="usagePanelMounted"
      v-show="activeView === 'usage'"
      :active="activeView === 'usage'"
    />

    <ApiServicePanel
      v-if="activeView === 'apiService'"
      :accounts="accounts"
      :settings="settings"
      @account-added="loadAccounts"
    />

    <SettingsPanel
      v-if="activeView === 'settings'"
      :settings="settings"
      :app-paths="appPaths"
      :backups="backupFiles"
      :loading="settingsLoading"
      :saving="savingSettings"
      :backup-loading="backupLoading"
      :backup-working="backupWorking"
      :backup-progress="backupProgress"
      @save="saveSettings"
      @open-path="openSessionFolder"
      @reset-config="confirmResetConfig"
      @export-backup="handleExportBackup"
      @refresh-backups="loadBackups"
      @restore-backup="handleRestoreBackup"
      @delete-backup="handleDeleteBackup"
    />

    <section v-if="activeView === 'about'" class="about-panel">
      <a-card class="about-card" :bordered="false">
        <div class="about-hero">
          <span class="about-eyebrow">About</span>
          <h2>Codex Switcher</h2>
          <p>本地管理 Codex OAuth 与 API Key 登录态，支持账号切换、会话维护、使用统计和本地 API 服务。</p>
        </div>
        <div class="about-grid">
          <div class="about-metric">
            <span>当前版本</span>
            <strong>v{{ appVersion }}</strong>
          </div>
          <div class="about-metric">
            <span>项目主页</span>
            <strong>GitHub</strong>
          </div>
        </div>
        <div class="about-actions">
          <a-button type="primary" @click="openGithubProfile">
            <template #icon><icon-link /></template>
            打开 GitHub 主页
          </a-button>
        </div>
      </a-card>
    </section>

    <a-modal
      v-model:visible="sessionRestoreVisible"
      title="恢复会话数据"
      :footer="false"
      width="720px"
    >
      <a-spin :loading="sessionBackupLoading" dot>
        <div v-if="sessionBackupFiles.length" class="session-restore-list">
          <article v-for="backup in sessionBackupFiles" :key="backup.path" class="session-restore-item">
            <div class="session-restore-main">
              <strong>{{ backup.name }}</strong>
              <span>{{ backup.createdAt }}</span>
            </div>
            <a-button type="primary" :disabled="backupWorking" @click="handleRestoreSessionBackup(backup)">
              <template #icon><icon-import /></template>
              只恢复会话
            </a-button>
          </article>
        </div>
        <div v-else class="session-empty-state compact">
          <div class="session-empty-icon">
            <icon-archive />
          </div>
          <div class="session-empty-copy">
            <strong>还没有备份文件</strong>
            <span>先备份一次会话数据，之后就可以从这里只恢复会话。</span>
          </div>
          <div class="session-empty-actions">
            <a-button type="primary" :loading="backupWorking" @click="handleExportSessionBackup">
              <template #icon><icon-download /></template>
              立即备份
            </a-button>
          </div>
        </div>
      </a-spin>
    </a-modal>

    <a-modal
      v-model:visible="backupProgressVisible"
      :title="backupProgressTitle"
      :footer="false"
      :closable="true"
      :mask-closable="true"
      width="420px"
    >
      <div class="backup-progress-panel">
        <a-progress
          :percent="backupProgress / 100"
          :status="backupProgressStatus === 'failed' ? 'danger' : backupProgressStatus === 'completed' ? 'success' : 'normal'"
        />
        <div
          class="backup-progress-message"
          :class="{ failed: backupProgressStatus === 'failed' }"
        >
          {{ backupProgressMessage }}
        </div>
        <a-button
          v-if="backupProgressStatus !== 'running'"
          type="primary"
          long
          @click="backupProgressVisible = false"
        >
          关闭
        </a-button>
      </div>
    </a-modal>

    <a-modal
      v-model:visible="switchRepairVisible"
      title="Codex 会话不可见"
      :footer="false"
      width="860px"
      modal-class="repair-modal"
    >
      <div class="repair-body">
        <p class="repair-desc">
          检测到 Codex 已切换到 {{ switchRepairTargetName }}。由于官方机制，这类切换后原有会话可能不会自动显示，正在自动修复会话可见性。
        </p>
        <div class="repair-progress-line">
          <strong>修复进度</strong>
          <span>{{ switchRepairProgress }}%</span>
        </div>
        <a-progress :percent="switchRepairProgress" :show-text="false" />
        <div v-if="switchRepairResult" class="repair-result success">
          <strong>修复已完成</strong>
          <span>{{ switchRepairResult.message }}</span>
        </div>
        <div v-else-if="switchRepairError" class="repair-result error">
          <strong>修复失败</strong>
          <span>{{ switchRepairError }}</span>
        </div>
        <div class="form-actions">
          <a-button type="primary" @click="closeSwitchRepairModal">关闭</a-button>
        </div>
      </div>
    </a-modal>

    <a-modal
      v-model:visible="addModalVisible"
      title="接入新账号"
      :footer="false"
      width="820px"
      modal-class="add-account-modal"
    >
      <div class="add-account-intro">
        <div>
          <span class="modal-eyebrow">Account Setup</span>
          <h3>选择一种方式，把账号接到 Codex Switcher</h3>
        </div>
        <p>推荐使用浏览器授权；如果已经有本地 token、JSON 或 API Key，也可以直接导入。</p>
      </div>
      <a-tabs v-model:active-key="addTab" class="add-account-tabs" @change="handleAddTabChange">
        <a-tab-pane key="oauth" title="OAuth 授权">
          <div class="oauth-connect-layout">
            <aside class="oauth-guide-card">
              <span class="modal-eyebrow">Browser Flow</span>
              <h4>浏览器登录，自动带回授权结果</h4>
              <ul>
                <li>先生成一次性授权链接</li>
                <li>在浏览器完成 OpenAI 登录</li>
                <li>回调成功后应用会自动保存账号</li>
              </ul>
              <div class="oauth-guide-note">
                如果浏览器没有自动回到应用，可复制地址栏里的 localhost 回调地址继续。
              </div>
            </aside>
            <div class="modal-form oauth-form">
              <div v-if="oauthError" class="oauth-error">{{ oauthError }}</div>
              <div v-else-if="oauthCallbackReceived" class="oauth-success">
                回调已收到，正在写入账号；如果保存失败，可以点下方按钮重试。
              </div>
              <div class="oauth-primary-action">
                <a-button
                  type="primary"
                  long
                  size="large"
                  :loading="oauthPreparing"
                  @click="startOrOpenOAuthUrl"
                >
                  <template #icon><icon-globe /></template>
                  {{ oauthUrl ? "继续打开授权页" : "生成并打开授权页" }}
                </a-button>
                <a-button v-if="oauthUrl" @click="copyOAuthUrl">
                  <template #icon><icon-copy /></template>
                  复制链接
                </a-button>
              </div>
              <div class="oauth-link-block compact">
                <label>当前授权地址</label>
                <a-input v-model="oauthUrl" readonly placeholder="点击上方按钮后生成授权地址" />
              </div>
              <div class="oauth-manual-box">
                <div>
                  <strong>手动完成</strong>
                  <span>浏览器未自动返回时，把 localhost 回调地址粘贴到这里。</span>
                </div>
                <div class="oauth-url-row">
                  <a-input
                    v-model="oauthCallbackInput"
                    placeholder="http://localhost:1455/auth/callback?code=...&state=..."
                  />
                  <a-button
                    type="primary"
                    :loading="oauthCompleting"
                    :disabled="!oauthLoginId"
                    @click="handleOAuthCallbackSubmit"
                  >
                    <template #icon><icon-check /></template>
                    {{ oauthCallbackReceived && !oauthCallbackInput.trim() ? "重试保存" : "完成接入" }}
                  </a-button>
                </div>
              </div>
            </div>
          </div>
        </a-tab-pane>
        <a-tab-pane key="token" title="Token / JSON">
          <div class="modal-form">
            <a-typography-paragraph>
              粘贴 session JSON、auth.json、账号 JSON、accessToken 或 refresh_token。
            </a-typography-paragraph>
            <div class="local-import-actions">
              <a-button type="primary" :loading="importing" @click="handleLocalImport">
                <template #icon><icon-folder /></template>
                获取本地账号
              </a-button>
              <a-button :loading="importing" @click="openFileImport">
                <template #icon><icon-import /></template>
                从本地文件导入
              </a-button>
              <input
                ref="fileInput"
                type="file"
                accept=".json,application/json"
                multiple
                class="hidden-file-input"
                @change="handleFileImport"
              />
            </div>
            <a-textarea
              v-model="tokenInput"
              class="token-textarea"
              :auto-size="{ minRows: 7, maxRows: 12 }"
              placeholder='示例：{"tokens":{"access_token":"eyJ...","refresh_token":"rt_..."}}'
            />
            <div class="form-actions">
              <a-button @click="addModalVisible = false">取消</a-button>
              <a-button type="primary" :loading="importing" @click="handleTokenImport">
                <template #icon><icon-import /></template>
                导入
              </a-button>
            </div>
          </div>
        </a-tab-pane>
        <a-tab-pane key="apikey" title="API Key">
          <div class="modal-form">
            <a-form :model="apiKeyForm" layout="vertical">
              <a-form-item label="账号名称">
                <a-input v-model="apiKeyForm.accountName" placeholder="例如：本地 codex 代理" />
              </a-form-item>
              <a-form-item label="供应商">
                <a-input v-model="apiKeyForm.apiProviderName" placeholder="OpenAI Official" />
              </a-form-item>
              <a-form-item label="Base URL">
                <a-input v-model="apiKeyForm.apiBaseUrl" placeholder="https://api.openai.com/v1" />
              </a-form-item>
              <a-form-item label="官网地址">
                <a-input v-model="apiKeyForm.apiOfficialUrl" placeholder="https://platform.openai.com" />
              </a-form-item>
              <a-form-item label="API Key">
                <a-input-password
                  v-model="apiKeyForm.apiKey"
                  autocomplete="new-password"
                  placeholder="sk-..."
                />
              </a-form-item>
              <a-form-item v-if="oauthAccounts.length" label="绑定已有 OAuth 账号">
                <a-select
                  v-model="apiKeyForm.boundOauthAccountId"
                  allow-clear
                  placeholder="可选：用于保留 Codex 会话身份"
                >
                  <a-option
                    v-for="oauth in oauthAccounts"
                    :key="oauth.id"
                    :value="oauth.id"
                  >
                    {{ displayNameForUi(oauth) }}
                  </a-option>
                </a-select>
              </a-form-item>
            </a-form>
            <div class="form-actions">
              <a-button @click="addModalVisible = false">取消</a-button>
              <a-button type="primary" :loading="savingApiKey" @click="handleApiKeyAdd">
                <template #icon><icon-plus /></template>
                添加
              </a-button>
            </div>
          </div>
        </a-tab-pane>
      </a-tabs>
    </a-modal>

    <a-modal
      v-model:visible="editVisible"
      :title="editTitle"
      :footer="false"
      width="760px"
    >
      <div class="modal-form">
        <a-tabs v-model:active-key="editTab">
          <a-tab-pane key="form" title="表单">
            <a-form :model="editForm" layout="vertical">
              <a-form-item label="账号名称">
                <a-input v-model="editForm.accountName" placeholder="例如：主力账号" />
              </a-form-item>
              <a-form-item v-if="editingAccount && isApiKeyAccount(editingAccount)" label="供应商">
                <a-input v-model="editForm.apiProviderName" placeholder="OpenAI Official" />
              </a-form-item>
              <a-form-item v-if="editingAccount && isApiKeyAccount(editingAccount)" label="Base URL">
                <a-input v-model="editForm.apiBaseUrl" placeholder="https://api.openai.com/v1" />
              </a-form-item>
              <a-form-item v-if="editingAccount && isApiKeyAccount(editingAccount)" label="官网地址">
                <a-input v-model="editForm.apiOfficialUrl" placeholder="https://platform.openai.com" />
              </a-form-item>
              <a-form-item v-if="editingAccount && isApiKeyAccount(editingAccount)" label="API Key">
                <a-input-password
                  v-model="editForm.apiKey"
                  autocomplete="new-password"
                  placeholder="sk-..."
                />
              </a-form-item>
            </a-form>
          </a-tab-pane>
          <a-tab-pane key="json" title="JSON">
            <a-textarea
              v-model="editJsonText"
              class="token-textarea json-edit-area"
              :auto-size="{ minRows: 12, maxRows: 20 }"
            />
          </a-tab-pane>
        </a-tabs>
        <div class="form-actions">
          <a-button @click="editVisible = false">取消</a-button>
          <a-button type="primary" :loading="editing" @click="handleEditSave">
            <template #icon><icon-save /></template>
            保存
          </a-button>
        </div>
      </div>
    </a-modal>

    <a-modal
      v-model:visible="exportVisible"
      title="导出 JSON"
      :footer="false"
      width="760px"
    >
      <div class="modal-form">
        <div class="export-toolbar">
          <div class="export-format">
            <span>导出格式</span>
            <a-select
              v-model="exportFormat"
              size="large"
              @change="refreshExportText"
            >
              <a-option
                v-for="option in exportFormatOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </a-option>
            </a-select>
          </div>
          <div>
            <a-button @click="exportPreviewVisible = !exportPreviewVisible">
              <template #icon><icon-eye /></template>
              {{ exportPreviewVisible ? "隐藏预览" : "预览" }}
            </a-button>
            <a-button @click="copyExportText">
              <template #icon><icon-copy /></template>
              复制
            </a-button>
            <a-button type="primary" @click="downloadExportText">
              <template #icon><icon-download /></template>
              下载
            </a-button>
          </div>
        </div>
        <a-textarea
          :model-value="exportPreviewVisible ? exportText : exportJsonSummary(exportText)"
          class="token-textarea export-json-viewer"
          :class="{ collapsed: !exportPreviewVisible }"
          readonly
          :auto-size="exportPreviewVisible ? { minRows: 14, maxRows: 24 } : { minRows: 12, maxRows: 12 }"
        />
      </div>
    </a-modal>

    <a-modal
      v-model:visible="batchExportVisible"
      title="批量导出 JSON"
      :footer="false"
      width="820px"
    >
      <div class="modal-form">
        <div class="export-toolbar">
          <div class="export-format">
            <span>导出格式</span>
            <a-select
              v-model="exportFormat"
              size="large"
              @change="refreshBatchExportText"
            >
              <a-option
                v-for="option in exportFormatOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </a-option>
            </a-select>
          </div>
          <div>
            <a-button @click="batchExportPreviewVisible = !batchExportPreviewVisible">
              <template #icon><icon-eye /></template>
              {{ batchExportPreviewVisible ? "隐藏预览" : "预览" }}
            </a-button>
            <a-button type="primary" @click="copyBatchExportText">
              <template #icon><icon-copy /></template>
              复制
            </a-button>
            <a-button type="primary" @click="downloadBatchExportText">
              <template #icon><icon-download /></template>
              下载
            </a-button>
          </div>
        </div>
        <a-textarea
          :model-value="batchExportPreviewVisible ? batchExportText : exportJsonSummary(batchExportText)"
          class="token-textarea export-json-viewer"
          :class="{ collapsed: !batchExportPreviewVisible }"
          readonly
          :auto-size="batchExportPreviewVisible ? { minRows: 14, maxRows: 24 } : { minRows: 12, maxRows: 12 }"
        />
      </div>
    </a-modal>

    <a-modal
      v-model:visible="phoneVisible"
      title="绑定手机"
      :footer="false"
      width="560px"
    >
      <div class="modal-form">
        <a-typography-paragraph v-if="phoneAccount">
          给 {{ displayNameForUi(phoneAccount) }} 保存一个绑定手机号，后续会直接显示在账号卡片上。
        </a-typography-paragraph>
        <a-form :model="phoneForm" layout="vertical">
          <a-form-item label="手机号">
            <a-input v-model="phoneForm.phone" placeholder="+1 (724) 806-2018" />
          </a-form-item>
        </a-form>
        <div class="form-actions">
          <a-button @click="phoneVisible = false">取消</a-button>
          <a-button type="primary" :loading="savingPhone" @click="handlePhoneSave">
            <template #icon><icon-save /></template>
            保存
          </a-button>
        </div>
      </div>
    </a-modal>

    <a-modal
      v-model:visible="bindingVisible"
      title="绑定 OAuth 账号"
      :footer="false"
      width="840px"
    >
      <div class="modal-form">
        <a-typography-paragraph>
          API Key 账号绑定 OAuth 后，切换时会同时写入 OAuth Token 与 API Key 配置，便于修复会话身份。
        </a-typography-paragraph>
        <div class="oauth-bind-list">
          <button
            type="button"
            class="oauth-bind-card unlink"
            :class="{ selected: !bindingForm.boundOauthAccountId }"
            @click="bindingForm.boundOauthAccountId = ''"
          >
            <span class="oauth-bind-check">
              <icon-check v-if="!bindingForm.boundOauthAccountId" />
            </span>
            <div class="oauth-bind-option-title">
              <strong>不绑定 OAuth</strong>
              <span>切换时仅写入 API Key 配置</span>
            </div>
          </button>

          <button
            v-for="oauth in oauthAccounts"
            :key="oauth.id"
            type="button"
            class="oauth-bind-card"
            :class="{ selected: bindingForm.boundOauthAccountId === oauth.id }"
            @click="bindingForm.boundOauthAccountId = oauth.id"
          >
            <span class="oauth-bind-check">
              <icon-check v-if="bindingForm.boundOauthAccountId === oauth.id" />
            </span>
            <div class="oauth-bind-option">
              <div class="oauth-bind-option-head">
                <div class="oauth-bind-option-title">
                  <strong>{{ displayNameForUi(oauth) }}</strong>
                  <span>OAuth · {{ oauth.email || oauth.id }}</span>
                </div>
                <PlanBadge :label="planLabel(oauth)" :badge-class="planClass(oauth)" />
              </div>
              <div v-if="oauth.quota" class="oauth-bind-quota">
                <div v-if="oauth.quota.hourly_window_present !== false">
                  <span>
                    <icon-calendar v-if="isFreePlanAccount(oauth)" />
                    <icon-clock-circle v-else />
                    {{ isFreePlanAccount(oauth) ? "长周期" : "短周期" }}
                  </span>
                  <strong :style="{ color: quotaColor(oauth.quota.hourly_percentage) }">
                    {{ oauth.quota.hourly_percentage }}%
                  </strong>
                  <small>{{ quotaWindowLabel(oauth.quota.hourly_window_minutes, '5 小时窗口') }}</small>
                  <em>{{ quotaResetLabel(oauth.quota.hourly_reset_time) }}</em>
                </div>
                <div
                  v-if="!isFreePlanAccount(oauth) && oauth.quota.weekly_window_present !== false"
                >
                  <span><icon-calendar /> 长周期</span>
                  <strong :style="{ color: quotaColor(oauth.quota.weekly_percentage) }">
                    {{ oauth.quota.weekly_percentage }}%
                  </strong>
                  <small>{{ quotaWindowLabel(oauth.quota.weekly_window_minutes, '7 天窗口') }}</small>
                  <em>{{ quotaResetLabel(oauth.quota.weekly_reset_time) }}</em>
                </div>
              </div>
              <div v-else-if="oauth.quota_error" class="oauth-bind-quota-error">
                {{ oauth.quota_error.message }}
              </div>
            </div>
          </button>
          <a-empty v-if="!oauthAccounts.length" description="暂无可绑定的 OAuth 账号" />
        </div>
        <div class="form-actions">
          <a-button @click="bindingVisible = false">取消</a-button>
          <a-button type="primary" :loading="savingBinding" @click="handleBindingSave">
            <template #icon><icon-save /></template>
            保存
          </a-button>
        </div>
      </div>
    </a-modal>

    <a-modal
      v-model:visible="repairVisible"
      title="找回会话显示"
      :footer="false"
      width="900px"
      modal-class="repair-modal"
    >
      <div class="repair-body">
        <div class="repair-hero">
          <div>
            <span class="modal-eyebrow">Session Recovery</span>
            <h3>把切号后消失的会话重新挂回列表</h3>
            <p>
              会同步整理 Codex 本地索引与状态库，让侧边栏重新识别已有会话；适合 OAuth 和 API Key 之间切换后使用。
            </p>
          </div>
          <div class="repair-summary-card">
            <strong>{{ selectedSessionIdList.length || sessions.length }}</strong>
            <span>{{ selectedSessionIdList.length ? "条已选会话" : "条可处理会话" }}</span>
          </div>
        </div>

        <div class="repair-section repair-section-inline">
          <div class="repair-section-copy">
            <span class="repair-section-title">处理强度</span>
            <small>优先使用轻量模式；仍然看不到再切到完整重建。</small>
          </div>
          <div class="repair-card-grid">
            <button
              class="repair-option-card"
              :class="{ selected: repairMode === 'quick' }"
              type="button"
              @click="repairMode = 'quick'"
            >
              <strong>轻量同步</strong>
              <small>更新状态库并补齐缺失记录，速度更快。</small>
            </button>
            <button
              class="repair-option-card"
              :class="{ selected: repairMode === 'deep' }"
              type="button"
              @click="repairMode = 'deep'"
            >
              <strong>完整重建</strong>
              <small>额外重写 session_index，适合普通同步无效时。</small>
            </button>
          </div>
        </div>

        <div class="repair-control-panel">
          <div class="repair-section">
            <span class="repair-section-title">Codex 实例</span>
            <a-select v-model="repairTargetInstanceId" placeholder="默认实例">
              <a-option
                v-for="instance in repairInstances"
                :key="instance.id"
                :value="instance.id"
              >
                {{ instance.name }} · {{ instance.currentProvider }}
              </a-option>
            </a-select>
          </div>
          <div class="repair-section">
            <span class="repair-section-title">实例覆盖</span>
            <div class="repair-segmented">
              <button
                :class="{ selected: repairInstanceScope === 'target' }"
                type="button"
                @click="repairInstanceScope = 'target'"
              >
                当前实例
              </button>
              <button
                :class="{ selected: repairInstanceScope === 'all' }"
                type="button"
                @click="repairInstanceScope = 'all'"
              >
                本机全部
              </button>
            </div>
          </div>
          <div class="repair-section">
            <span class="repair-section-title">会话覆盖</span>
            <div class="repair-segmented">
              <button
                :class="{ selected: effectiveRepairSessionScope === 'all' }"
                type="button"
                @click="repairSessionScope = 'all'"
              >
                全部 {{ sessions.length }}
              </button>
              <button
                :class="{ selected: effectiveRepairSessionScope === 'selected' }"
                :disabled="!selectedSessionIdList.length"
                type="button"
                @click="repairSessionScope = 'selected'"
              >
                已选 {{ selectedSessionIdList.length }}
              </button>
            </div>
          </div>
        </div>

        <div v-if="repairResult" class="repair-result">
          <strong>{{ repairResult.message }}</strong>
          <span v-if="repairResult.changedRolloutFileCount !== undefined">
            会话文件 {{ repairResult.changedRolloutFileCount }} 个
          </span>
          <span>SQLite {{ repairResult.updatedSqliteRowCount ?? repairResult.repaired }} 条</span>
          <span v-if="repairResult.updatedSqliteTimestampRowCount">
            时间记录 {{ repairResult.updatedSqliteTimestampRowCount }} 条
          </span>
          <span v-if="repairResult.addedSessionIndexEntryCount">
            session_index {{ repairResult.addedSessionIndexEntryCount }} 条
          </span>
        </div>

        <div class="form-actions">
          <a-button @click="repairVisible = false">关闭</a-button>
          <a-button type="primary" :loading="sessionRepairing" @click="runRepairSessions">
            <template #icon><icon-refresh /></template>
            立即找回
          </a-button>
        </div>
      </div>
    </a-modal>
  </main>
</template>
