<script setup lang="ts">
import {
  computed,
  defineAsyncComponent,
  nextTick,
  onMounted,
  onUnmounted,
  reactive,
  ref,
  watch,
} from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import { getVersion } from "@tauri-apps/api/app";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isSubscriptionExpired } from "./accountStatus";
import AccountList from "./components/AccountList.vue";
import AboutPanel from "./components/AboutPanel.vue";
import AccountToolbar from "./components/AccountToolbar.vue";
import AddAccountModal from "./components/AddAccountModal.vue";
import ApiKeyModelModal from "./components/ApiKeyModelModal.vue";
import AppUpdateModal from "./components/AppUpdateModal.vue";
import AppHeader from "./components/AppHeader.vue";
import BackupProgressModal from "./components/BackupProgressModal.vue";
import BadgeStyleModal from "./components/BadgeStyleModal.vue";
import CodexConfigEditorModal from "./components/CodexConfigEditorModal.vue";
import EditAccountModal from "./components/EditAccountModal.vue";
import ExportJsonModal from "./components/ExportJsonModal.vue";
import OAuthBindingModal from "./components/OAuthBindingModal.vue";
import PhoneModal from "./components/PhoneModal.vue";
import PushSettingsPanel from "./components/PushSettingsPanel.vue";
import ResetCreditModal from "./components/ResetCreditModal.vue";
import ResetPanel from "./components/ResetPanel.vue";
import ResetScheduleModal from "./components/ResetScheduleModal.vue";
import SessionCopyModal from "./components/SessionCopyModal.vue";
import SessionContentModal from "./components/SessionContentModal.vue";
import SessionDirectoryModal from "./components/SessionDirectoryModal.vue";
import SessionPanel from "./components/SessionPanel.vue";
import SessionRepairModal from "./components/SessionRepairModal.vue";
import SessionRenameModal from "./components/SessionRenameModal.vue";
import SessionRestoreModal from "./components/SessionRestoreModal.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import SortEditorModal from "./components/SortEditorModal.vue";
import { defaultBadgeStyles } from "./constants/badgeStyles";
import {
  currentLanguage,
  formatLocalizedDuration,
  formatTranslatedText,
  setLanguage,
  t,
} from "./i18n";
import { additionalQuotaWindows, hasQuotaWindow, quotaWindowForMinutes } from "./quota";
import { resolveResetScheduleEntry } from "./services/resetScheduleEntry";
import {
  beginPendingItem,
  finishPendingItem,
  hasAvailableResetCredit,
} from "./services/resetUiState";
import {
  addCodexAccountWithApiKey,
  cancelCodexOAuthLogin,
  checkCodexApiKeyModelAccess,
  completeCodexOAuthLogin,
  consumeCodexResetCredit,
  deleteCodexAccount,
  deleteCodexSwitcherBackup,
  detectCurrentCodexAccount,
  exportCodexAccounts,
  fetchCodexApiKeyBalance,
  fetchCodexApiKeyModels,
  formatCodexConfigFile,
  getCodexSwitcherPaths,
  getCodexSwitcherSettings,
  getCurrentCodexAccount,
  importCodexFromJson,
  importCodexFromLocal,
  readCodexConfigFile,
  listCodexSwitcherBackups,
  listCodexSwitcherSessionBackups,
  listCodexAccounts,
  openExternalUrl,
  refreshAllCodexQuotas,
  refreshCodexQuota,
  resetCodexConfigToml,
  restoreCodexSwitcherBackup,
  restoreCodexSwitcherSessionBackup,
  restartCodexApp,
  startCodexSwitcherBackup,
  startCodexSwitcherSessionBackup,
  startCodexOAuthLogin,
  setCodexApiKeyDefaultModel,
  switchCodexAccount,
  submitCodexOAuthCallbackUrl,
  updateCodexSwitcherSettings,
  writeCodexConfigFile,
  updateCodexAccountPhone,
  updateCodexAccountFromJson,
  updateCodexAccountProfile,
  updateCodexApiKeyBoundOAuthAccount,
  updateCodexApiKeyCredentials,
  type CodexExportFormat,
  type CodexApiKeyBalanceState,
  type CodexApiKeyModel,
  type CodexConfigFileContent,
  type CodexConfigFileKind,
  type CodexSwitcherBackupFile,
  type CodexSwitcherBackupProgressEvent,
  type CodexSwitcherPaths,
  type CodexSwitcherSettings,
} from "./services/codex";
import {
  API_SERVICE_AUTO_UPDATE_EVENT,
  bindApiServiceAccounts,
  getApiServiceState,
  isCurrentApiServiceAccount,
  listApiServiceBoundAccounts,
  type ApiServiceAutoUpdateEvent,
} from "./services/apiService";
import {
  APP_UPDATE_DOWNLOAD_PROGRESS_EVENT,
  cancelAppUpdateDownload,
  downloadAppUpdate,
  fetchAppUpdateInfo,
  openAppUpdateInstaller,
  type AppUpdateDownloadProgress,
  type AppUpdateDownloadResult,
  type AppUpdateInfo,
} from "./services/appUpdate";
import {
  copySessionHistoryAcrossInstances,
  listSessionVisibilityRepairInstances,
  listSessionVisibilityRepairProviders,
  listSessionsAcrossInstances,
  listTrashedSessionsAcrossInstances,
  moveSessionsToTrashAcrossInstances,
  openPathInFileManager,
  repairSessionVisibilityAcrossInstances,
  renameSessionAcrossInstances,
  restoreSessionsFromTrashAcrossInstances,
  updateSessionWorkingDirectoryAcrossInstances,
  type CodexSessionRecord,
  type CodexSessionTokenStats,
  type CodexSessionVisibilityRepairInstanceOption,
  type CodexSessionVisibilityRepairMode,
  type CodexSessionVisibilityRepairProviderOption,
  type CodexSessionVisibilityRepairSummary,
  type CodexTrashedSessionRecord,
} from "./services/session";
import {
  appendCodexResetLog,
  cancelCodexScheduledReset,
  claimCodexScheduledReset,
  clearCodexResetLogs,
  createCodexScheduledReset,
  deleteCodexResetLog,
  finishCodexScheduledReset,
  formatResetCountdown,
  getCodexResetState,
  initializeCodexResetState,
  updateCodexScheduledReset,
  type ResetState,
  type ScheduledReset,
} from "./services/reset";
import type { CodexAccount, CodexResetCredit } from "./types/codex";
import type { ActiveView, SessionGroup } from "./types/ui";

const ApiServicePanel = defineAsyncComponent(() => import("./components/ApiServicePanel.vue"));
const UsagePanel = defineAsyncComponent(() => import("./components/UsagePanel.vue"));

const activeView = ref<ActiveView>("accounts");
const appVersion = ref("0.1.0");
const checkingAppUpdate = ref(false);
const appUpdateVisible = ref(false);
const appUpdateInfo = ref<AppUpdateInfo | null>(null);
const appUpdateProgress = ref<AppUpdateDownloadProgress | null>(null);
const appUpdateResult = ref<AppUpdateDownloadResult | null>(null);
const appUpdateError = ref("");
const appUpdateDownloading = ref(false);
const appUpdateCancelling = ref(false);
const appUpdateOpening = ref(false);
const usagePanelMounted = ref(false);
const apiServicePanelMounted = ref(false);
const apiServiceAutoUpdateEvent = ref<ApiServiceAutoUpdateEvent | null>(null);
const apiServiceAccountIds = ref<Set<string>>(new Set());
const accounts = ref<CodexAccount[]>([]);
const currentAccount = ref<CodexAccount | null>(null);
const loading = ref(false);
const detectingCurrentAccount = ref(false);
const accountSearchKeyword = ref("");
const switchingId = ref("");
const deletingId = ref("");
const quotaRefreshingId = ref("");
const apiKeyBalanceStates = reactive<Record<string, CodexApiKeyBalanceState>>({});
const apiKeyBalanceRequestSequences = new Map<string, number>();
let apiKeyBalanceRequestSequence = 0;
const apiKeyBalanceLastAttemptAt = new Map<string, number>();
const apiKeyBalanceInsecureHttpApprovals = new Map<string, string>();
const apiKeyBalanceInsecureHttpConfirmationPending = new Set<string>();
const INSECURE_HTTP_CONFIRM_REQUIRED = "INSECURE_HTTP_CONFIRM_REQUIRED:";
const API_KEY_BALANCE_TTL_MS = 5 * 60_000;
const API_KEY_BALANCE_PREFETCH_CONCURRENCY = 3;
type ApiKeyBalancePrefetchTask = { account: CodexAccount; generation: number };
const apiKeyBalancePrefetchQueue: ApiKeyBalancePrefetchTask[] = [];
let apiKeyBalancePrefetchGeneration = 0;
let apiKeyBalancePrefetchWorkers = 0;
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
let apiKeyBalanceRefreshTimer: number | undefined;
let scheduledQuotaRefreshRunning = false;
let scheduledCurrentAccountRefreshRunning = false;
let accountLoadRequested = false;
let accountLoadPromise: Promise<void> | undefined;
const nextQuotaRefreshAt = ref(0);
const nextCurrentAccountRefreshAt = ref(0);
const nowMs = ref(Date.now());
const accountStatusClockMs = computed(() => Math.floor(nowMs.value / 5_000) * 5_000);
const addModalVisible = ref(false);
const addModalTitle = ref("接入新账号");
const badgeStyleVisible = ref(false);
const privacyMasked = ref(false);
const addTab = ref<"oauth" | "token" | "apikey">("oauth");
const tokenInput = ref("");
const importing = ref(false);
const savingApiKey = ref(false);
const refreshingAllQuotas = ref(false);
const settingsLoading = ref(false);
const savingSettings = ref(false);
const configEditorVisible = ref(false);
const configEditorKind = ref<CodexConfigFileKind>("auth");
const configEditorFile = ref<CodexConfigFileContent | null>(null);
const configEditorContent = ref("");
const configEditorLoading = ref(false);
const configEditorSaving = ref(false);
const configEditorFormatting = ref(false);
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
  backupWorking.value ? t(`备份 ${Math.round(backupProgress.value)}%`) : t("备份"),
);
const sessionRestoreVisible = ref(false);
const expandedLayout = ref(false);
let windowResizeFrame: number | undefined;
let viewLoadTimer: number | undefined;
const EXPANDED_LAYOUT_MIN_WIDTH = 1260;
const settings = reactive<CodexSwitcherSettings>({
  monitorQuota: true,
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
  accountViewMode: "card",
  sidebarEnabled: true,
  showQuotaCountdowns: true,
  showAdditionalQuotaWindows: true,
  badgeStyle: "classic",
  badgeStyles: defaultBadgeStyles(),
  maxColumns: 5,
  language: "zh-CN",
});

const oauthLoginId = ref("");
const oauthUrl = ref("");
const oauthCallbackInput = ref("");
const oauthPreparing = ref(false);
const oauthCompleting = ref(false);
const oauthError = ref("");
const oauthCallbackReceived = ref(false);
let oauthUnlisten: UnlistenFn | null = null;
let appUpdateUnlisten: UnlistenFn | null = null;
let apiServiceAutoUpdateUnlisten: UnlistenFn | null = null;
let accountStateUnlisten: UnlistenFn | null = null;

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
  tags: [] as string[],
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
const resetCreditVisible = ref(false);
const resetScheduleVisible = ref(false);
const resetCreditAccount = ref<CodexAccount | null>(null);
const resetState = ref<ResetState>({ scheduledResets: [], logs: [] });
const resetStateLoading = ref(false);
const resetStateSaving = ref(false);
const editingResetSchedule = ref<ScheduledReset | null>(null);
const updatingResetScheduleIds = ref<string[]>([]);
const cancellingResetScheduleIds = ref<string[]>([]);
const deletingResetLogIds = ref<string[]>([]);
const clearingResetLogs = ref(false);
let resetExecutionPromise: Promise<void> | undefined;
let resetStateLoadPromise: Promise<void> | undefined;
let resetStateOperationTail: Promise<void> = Promise.resolve();
let resetStatePendingMutations = 0;

const bindingVisible = ref(false);
const bindingAccount = ref<CodexAccount | null>(null);
const bindingForm = reactive({ boundOauthAccountId: "" });
const savingBinding = ref(false);
const apiModelVisible = ref(false);
const apiModelAccount = ref<CodexAccount | null>(null);
const apiModels = ref<CodexApiKeyModel[]>([]);
const selectedApiModel = ref("");
const fetchingApiModels = ref(false);
const savingApiModel = ref(false);
const apiModelAccessStatus = ref<"idle" | "checking" | "matched" | "mismatched" | "error">(
  "idle",
);
const apiModelAccessError = ref("");
let apiModelAccessSequence = 0;
let apiModelRequestSequence = 0;
let apiModelSaveSequence = 0;

const sessions = ref<CodexSessionRecord[]>([]);
const trashedSessions = ref<CodexTrashedSessionRecord[]>([]);
const sessionStats = ref<CodexSessionTokenStats[]>([]);
const selectedSessionIds = ref<Set<string>>(new Set());
const expandedSessionGroups = ref<Set<string>>(new Set());
const sessionLoading = ref(false);
const sessionRepairing = ref(false);
const repairVisible = ref(false);
const repairMode = ref<CodexSessionVisibilityRepairMode>("quick");
const repairInstanceScope = ref<"target" | "all">("target");
const repairSessionScope = ref<"all" | "selected">("all");
const repairTargetInstanceId = ref("__default__");
const repairInstances = ref<CodexSessionVisibilityRepairInstanceOption[]>([]);
const repairProviders = ref<CodexSessionVisibilityRepairProviderOption[]>([]);
const repairResult = ref<CodexSessionVisibilityRepairSummary | null>(null);
const sessionTrashMode = ref(false);
const sessionCopyVisible = ref(false);
const sessionCopySource = ref<CodexSessionRecord | null>(null);
const sessionCopySaving = ref(false);
const sessionContentVisible = ref(false);
const sessionContentTarget = ref<CodexSessionRecord | null>(null);
const sessionRenameVisible = ref(false);
const sessionRenameTarget = ref<CodexSessionRecord | null>(null);
const sessionRenameSaving = ref(false);
const sessionDirectoryVisible = ref(false);
const sessionDirectoryTarget = ref<CodexSessionRecord | null>(null);
const sessionDirectorySaving = ref(false);
const sessionSearch = reactive({
  titleQuery: "",
  contentQuery: "",
});
let sessionLoadSequence = 0;

const currentId = computed(() => currentAccount.value?.id ?? "");
const quotaSortModes = new Set([
  "weekly_quota",
  "hourly_quota",
  "quota_reset_countdown",
  "weekly_reset",
  "hourly_reset",
  "subscription",
  "tags",
]);
const filteredAccounts = computed(() => {
  const filter = settings.accountTypeFilter || "all";
  const keyword = accountSearchKeyword.value.trim().toLocaleLowerCase();
  return accounts.value.filter((account) => {
    const matchesType =
      filter === "all" ||
      (filter === "oauth" && !isApiKeyAccount(account)) ||
      (filter === "apikey" && isApiKeyAccount(account)) ||
      (filter === "error" && isAccountAbnormal(account)) ||
      (filter === "valid" && !isAccountAbnormal(account)) ||
      (filter === "pro" && effectivePlanKey(account) === "pro") ||
      (filter === "team" && ["team", "business", "enterprise", "edu", "go"].includes(effectivePlanKey(account))) ||
      (filter.startsWith("tag:") && accountTags(account).includes(filter.slice(4))) ||
      effectivePlanKey(account) === filter;
    if (!matchesType) return false;
    if (!keyword) return true;
    return [account.email, account.account_name, ...accountTags(account)]
      .some((value) => (value || "").toLocaleLowerCase().includes(keyword));
  });
});
function sortAccountsForDisplay(source: CodexAccount[]): CodexAccount[] {
  const order = new Map(settings.customOrder.map((id, index) => [id, index]));
  const pinned = new Map((settings.pinnedAccountIds || []).map((id, index) => [id, index]));
  const sourcePosition = new Map(source.map((account, index) => [account.id, index]));
  const sortDirection = settings.sortDirection === "asc" ? 1 : -1;
  const sortValue = (account: CodexAccount): number => {
    switch (settings.sortMode) {
      case "weekly_quota":
        return quotaWindowForMinutes(account.quota, 10_080)?.percentage
          ?? Number.NEGATIVE_INFINITY;
      case "hourly_quota":
        return quotaWindowForMinutes(account.quota, 300)?.percentage
          ?? Number.NEGATIVE_INFINITY;
      case "quota_reset_countdown":
        return nearestQuotaResetTime(account) ?? Number.POSITIVE_INFINITY;
      case "weekly_reset":
        return quotaWindowForMinutes(account.quota, 10_080)?.resetTime
          ?? Number.NEGATIVE_INFINITY;
      case "hourly_reset":
        return quotaWindowForMinutes(account.quota, 300)?.resetTime
          ?? Number.NEGATIVE_INFINITY;
      case "subscription":
        return dateSortValue(accountSubscriptionUntil(account));
      case "custom":
        return order.get(account.id) ?? Number.MAX_SAFE_INTEGER;
      case "tags":
      case "created_at":
      default:
        return account.created_at;
    }
  };
  return [...source].sort((a, b) => {
    const aPinned = pinned.get(a.id);
    const bPinned = pinned.get(b.id);
    if (aPinned !== undefined || bPinned !== undefined) {
      if (aPinned === undefined) return 1;
      if (bPinned === undefined) return -1;
      return aPinned - bPinned;
    }
    if (settings.sortMode !== "custom") {
      const groupDiff = settings.sortMode === "tags" ? 0 : accountSortGroup(a) - accountSortGroup(b);
      if (groupDiff !== 0) return groupDiff;
    }
    if (
      settings.sortMode !== "created_at" &&
      settings.sortMode !== "custom" &&
      isApiKeyAccount(a) &&
      isApiKeyAccount(b)
    ) {
      const createdDiff = a.created_at - b.created_at;
      if (createdDiff !== 0) return createdDiff;
      const insertionDiff =
        (sourcePosition.get(b.id) ?? Number.MAX_SAFE_INTEGER) -
        (sourcePosition.get(a.id) ?? Number.MAX_SAFE_INTEGER);
      if (insertionDiff !== 0) return insertionDiff;
      return a.id.localeCompare(b.id);
    }
    const left = sortValue(a);
    const right = sortValue(b);
    if (settings.sortMode === "quota_reset_countdown") {
      const leftFinite = Number.isFinite(left);
      const rightFinite = Number.isFinite(right);
      if (leftFinite !== rightFinite) return leftFinite ? -1 : 1;
    }
    if (settings.sortMode === "tags") {
      const aHasTags = accountTags(a).length > 0;
      const bHasTags = accountTags(b).length > 0;
      if (aHasTags !== bHasTags) return aHasTags ? -1 : 1;
      const tagDiff = accountTagSortKey(a).localeCompare(accountTagSortKey(b), currentLanguage.value);
      if (tagDiff !== 0) return tagDiff * sortDirection;
    }
    if (left !== right) {
      return settings.sortMode === "custom" ? left - right : (left - right) * sortDirection;
    }
    return b.last_used - a.last_used;
  });
}
const sortedAccounts = computed(() => sortAccountsForDisplay(filteredAccounts.value));
const apiServiceAccounts = computed(() => sortAccountsForDisplay(accounts.value));
const allAccountTags = computed(() => {
  currentLanguage.value;
  return [...new Set(accounts.value.flatMap(accountTags))].sort((a, b) =>
    a.localeCompare(b, currentLanguage.value),
  );
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
  currentLanguage.value;
  const count = (predicate: (account: CodexAccount) => boolean) => accounts.value.filter(predicate).length;
  const baseOptions = [
    { label: `${t("全部")} (${accounts.value.length})`, value: "all" },
    { label: `OAuth (${oauthCount.value})`, value: "oauth" },
    { label: `API Key (${apiKeyCount.value})`, value: "apikey" },
    { label: `FREE (${count((account) => !isApiKeyAccount(account) && effectivePlanKey(account) === "free")})`, value: "free" },
    { label: `PLUS (${count((account) => !isApiKeyAccount(account) && effectivePlanKey(account) === "plus")})`, value: "plus" },
    { label: `PRO (${count((account) => !isApiKeyAccount(account) && effectivePlanKey(account) === "pro")})`, value: "pro" },
    {
      label: `TEAM (${count((account) =>
        !isApiKeyAccount(account) && ["team", "business", "enterprise", "edu", "go"].includes(effectivePlanKey(account)),
      )})`,
      value: "team",
    },
    { label: `${t("异常")} (${count(isAccountAbnormal)})`, value: "error" },
    { label: `${t("有效账号")} (${count((account) => !isAccountAbnormal(account))})`, value: "valid" },
  ];
  const tagOptions = allAccountTags.value.map((tag) => ({
    label: `${t("标签")}：${tag} (${count((account) => accountTags(account).includes(tag))})`,
    value: `tag:${tag}`,
  }));
  return [...baseOptions, ...tagOptions];
});
const oauthCount = computed(
  () => accounts.value.filter((account) => !isApiKeyAccount(account)).length,
);
const apiKeyCount = computed(
  () => accounts.value.filter((account) => isApiKeyAccount(account)).length,
);
const abnormalAccountCount = computed(() => accounts.value.filter(isAccountAbnormal).length);
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
      projectName: session.projectName || t("未归属项目"),
      sessions: [],
      latestUpdatedAt: 0,
      approximateTokens: 0,
      sizeBytes: 0,
    };
    group.sessions.push(session);
    group.latestUpdatedAt = Math.max(group.latestUpdatedAt, session.updatedAt);
    group.approximateTokens += sessionStats.value.find((item) => item.sessionId === session.id)?.approximateTokens ?? 0;
    group.sizeBytes += session.sizeBytes || 0;
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
  return Math.max(1, Math.min(518400, Math.round(minutes)));
}

function normalizeAccountViewMode(value: unknown): CodexSwitcherSettings["accountViewMode"] {
  return value === "compact" || value === "table" ? value : "card";
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
      accountViewMode: normalizeAccountViewMode(settings.accountViewMode),
      sidebarEnabled: settings.sidebarEnabled ?? true,
      quotaRefreshMinutes: clampRefreshMinutes(settings.quotaRefreshMinutes),
      currentAccountRefreshMinutes: clampRefreshMinutes(settings.currentAccountRefreshMinutes),
      quotaNextRefreshAt: settings.monitorQuota ? Math.floor(settings.quotaNextRefreshAt || 0) : 0,
      currentAccountNextRefreshAt: settings.monitorQuota
        ? Math.floor(settings.currentAccountNextRefreshAt || 0)
        : 0,
      showQuotaCountdowns: settings.showQuotaCountdowns ?? true,
      showAdditionalQuotaWindows: settings.showAdditionalQuotaWindows ?? true,
      maxColumns: [3, 4, 5].includes(settings.maxColumns) ? settings.maxColumns : 5,
      language: settings.language || "zh-CN",
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
    return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function isApiKeyAccount(account: CodexAccount): boolean {
  return account.auth_mode === "apikey" || Boolean(account.openai_api_key || account.openaiApiKey);
}

function apiKeyAccountBaseUrl(account: CodexAccount): string {
  return (account.api_base_url || account.apiBaseUrl || "").trim();
}

function remoteHttpBalanceOrigin(account: CodexAccount): string | undefined {
  const baseUrl = apiKeyAccountBaseUrl(account);
  if (!baseUrl) return undefined;
  try {
    const url = new URL(baseUrl);
    if (url.protocol !== "http:") return undefined;
    const hostname = url.hostname
      .replace(/^\[/, "")
      .replace(/\]$/, "")
      .toLowerCase();
    if (hostname === "localhost" || hostname === "::1") return undefined;
    const ipv4 = hostname.split(".").map((part) => Number(part));
    const isIpv4Loopback =
      ipv4.length === 4 &&
      ipv4.every((part) => Number.isInteger(part) && part >= 0 && part <= 255) &&
      ipv4[0] === 127;
    return isIpv4Loopback ? undefined : url.origin;
  } catch {
    return undefined;
  }
}

function apiKeyBalanceCanAutoFetch(account: CodexAccount): boolean {
  const origin = remoteHttpBalanceOrigin(account);
  if (!origin || apiKeyBalanceInsecureHttpApprovals.get(account.id) === origin) return true;
  const previous = apiKeyBalanceStates[account.id];
  if (previous?.status !== "consent_required") {
    apiKeyBalanceStates[account.id] = {
      status: "consent_required",
      balance: previous?.balance,
      fetchedAt: previous?.fetchedAt || 0,
    };
  }
  return false;
}

function displayName(account: CodexAccount): string {
  return account.account_name || account.email || account.id;
}

function accountTags(account: CodexAccount): string[] {
  if (!Array.isArray(account.tags)) return [];
  const tags: string[] = [];
  for (const rawTag of account.tags) {
    const tag = String(rawTag || "").trim();
    if (tag && !tags.includes(tag)) tags.push(tag);
  }
  return tags;
}

function accountTagSortKey(account: CodexAccount): string {
  return accountTags(account)[0] || "\uffff";
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
  if (account.quota_error?.code === "token_expired") return t("Token 失效");
  if (account.quota_error) return t("额度异常");
  return "";
}

function boundOAuthName(account: CodexAccount): string {
  const bound = boundOAuthAccount(account);
  return bound ? displayName(bound) : t("未绑定");
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
  return session.projectPath || session.projectName || t("未归属项目");
}

function canShowQuota(account: CodexAccount): boolean {
  if (!settings.monitorQuota) return false;
  return !isApiKeyAccount(account);
}

function nearestQuotaResetTime(account: CodexAccount): number | undefined {
  const candidates = [
    hasQuotaWindow(account.quota, "hourly") ? account.quota?.hourly_reset_time : undefined,
    hasQuotaWindow(account.quota, "weekly") ? account.quota?.weekly_reset_time : undefined,
    ...additionalQuotaWindows(account.quota).map((window) => window.resetTime),
  ].filter((value): value is number => typeof value === "number" && Number.isFinite(value));
  return candidates.length ? Math.min(...candidates) : undefined;
}

function shouldShowQuota(account: CodexAccount): boolean {
  return canShowQuota(account) && Boolean(account.quota);
}

function shouldShowQuotaError(account: CodexAccount): boolean {
  return canShowQuota(account) && Boolean(account.quota_error);
}

function isAccountAbnormal(account: CodexAccount): boolean {
  if (account.quota_error) return true;
  if (
    !isApiKeyAccount(account) &&
    tokenExpiryStatus(accountTokenExpiresAt(account), accountStatusClockMs.value) === "expired"
  ) {
    return true;
  }
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
  const base = planDisplayName(effectivePlanKey(account));
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
  const key = effectivePlanKey(account);
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
  const key = effectivePlanKey(account);
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

function effectivePlanKey(account: CodexAccount): string {
  if (
    !isApiKeyAccount(account) &&
    isSubscriptionExpired(accountSubscriptionUntil(account), accountStatusClockMs.value)
  ) {
    return "free";
  }
  return normalizePlanKey(account.plan_type);
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
  if (!date) return t("未知");
  const diff = date.getTime() - Date.now();
  if (diff <= 0) return t("已过期");
  const totalMinutes = Math.floor(diff / 60_000);
  const days = Math.floor(totalMinutes / 1440);
  const hours = Math.floor((totalMinutes % 1440) / 60);
  const minutes = totalMinutes % 60;
  return formatLocalizedDuration(days, hours, minutes);
}

function tokenExpiryStatus(value?: string, referenceTime = Date.now()): "normal" | "expired" {
  const date = parseFlexibleDate(value);
  if (!date) return "normal";
  return date.getTime() <= referenceTime ? "expired" : "normal";
}

function dateSortValue(value?: string): number {
  const date = parseFlexibleDate(value);
  return date ? date.getTime() : Number.NEGATIVE_INFINITY;
}

function quotaWindowLabel(minutes?: number, fallback = "5h"): string {
  if (!minutes || !Number.isFinite(minutes)) return t(fallback);
  if (minutes % (60 * 24 * 7) === 0) return t(`${minutes / (60 * 24 * 7) * 7} 天窗口`);
  if (minutes % (60 * 24) === 0) return t(`${minutes / (60 * 24)} 天窗口`);
  if (minutes % 60 === 0) return t(`${minutes / 60} 小时窗口`);
  return t(`${minutes} 分钟窗口`);
}

function quotaResetLabel(timestamp?: number): string {
  if (!timestamp) return "";
  const diff = timestamp - Math.floor(Date.now() / 1000);
  if (diff <= 0) return t("已重置");
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

const resetCreditRecordsForModal = computed(() =>
  resetCreditAccount.value ? resetCreditRecords(resetCreditAccount.value) : [],
);

const hasAvailableResetCreditForModal = computed(() =>
  hasAvailableResetCredit(resetCreditRecordsForModal.value, isAvailableResetCredit),
);

const scheduledResetForModal = computed<ScheduledReset | null>(() => {
  const accountId = resetCreditAccount.value?.id;
  if (!accountId) return null;
  return (
    resetState.value.scheduledResets.find(
      (task) =>
        task.accountId === accountId &&
        (task.status === "scheduled" || task.status === "running"),
    ) ?? null
  );
});

const resetScheduleAccountLabel = computed(
  () =>
    editingResetSchedule.value?.accountLabel ||
    (resetCreditAccount.value ? displayNameForUi(resetCreditAccount.value) : ""),
);

const resetScheduleMode = computed<"create" | "edit">(() =>
  editingResetSchedule.value ? "edit" : "create",
);

const resetScheduleInitialAt = computed(() => editingResetSchedule.value?.scheduledAt);

function canUseResetCredit(account: CodexAccount): boolean {
  return shouldShowQuota(account) && resetCreditCount(account) > 0;
}

function resetCreditRecords(account: CodexAccount): CodexResetCredit[] {
  if (Array.isArray(account.quota?.reset_credits) && account.quota.reset_credits.length > 0) {
    return account.quota.reset_credits;
  }
  return parseResetCreditRecordsFromRawData(account.quota?.raw_data);
}

function parseResetCreditRecordsFromRawData(rawData: unknown): CodexResetCredit[] {
  const root = isRecord(rawData) ? rawData : undefined;
  const container = isRecord(root?.rate_limit_reset_credits)
    ? root.rate_limit_reset_credits
    : isRecord(root?.data) && isRecord(root.data.rate_limit_reset_credits)
      ? root.data.rate_limit_reset_credits
      : undefined;
  const credits = Array.isArray(container?.credits)
    ? container.credits
    : isRecord(container?.data) && Array.isArray(container.data.credits)
      ? container.data.credits
      : [];

  return credits.filter(isRecord).map((credit) => ({
    id: normalizeScalar(credit.id) || normalizeScalar(credit.credit_id) || normalizeScalar(credit.creditId),
    status: normalizeScalar(credit.status) || normalizeScalar(credit.state),
    reset_type: normalizeScalar(credit.type) || normalizeScalar(credit.reset_type) || normalizeScalar(credit.resetType),
    granted_at:
      normalizeTimestamp(credit.granted_at) ??
      normalizeTimestamp(credit.created_at) ??
      normalizeTimestamp(credit.grantedAt),
    expires_at:
      normalizeTimestamp(credit.expires_at) ??
      normalizeTimestamp(credit.expire_at) ??
      normalizeTimestamp(credit.expiresAt),
    redeemed_at:
      normalizeTimestamp(credit.redeemed_at) ??
      normalizeTimestamp(credit.used_at) ??
      normalizeTimestamp(credit.consumed_at) ??
      normalizeTimestamp(credit.redeemedAt),
    raw_status: normalizeScalar(credit.status) || normalizeScalar(credit.state),
  }));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function normalizeScalar(value: unknown): string | undefined {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed || undefined;
  }
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return undefined;
}

function normalizeTimestamp(value: unknown): number | undefined {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value > 1_000_000_000_000 ? Math.floor(value / 1000) : Math.floor(value);
  }
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  if (!trimmed) return undefined;
  const numeric = Number(trimmed);
  if (Number.isFinite(numeric)) {
    return numeric > 1_000_000_000_000 ? Math.floor(numeric / 1000) : Math.floor(numeric);
  }
  const date = new Date(trimmed);
  return Number.isNaN(date.getTime()) ? undefined : Math.floor(date.getTime() / 1000);
}

function resetCreditStatusKey(credit: CodexResetCredit): "available" | "used" | "expired" | "unknown" {
  const status = (credit.status || credit.raw_status || "").trim().toLowerCase();
  if (["redeemed", "used", "consumed"].includes(status)) return "used";
  if (status === "expired") return "expired";
  if (status === "available" || !status) {
    const expiresAt = credit.expires_at;
    return Number.isFinite(expiresAt) && Number(expiresAt) * 1000 <= Date.now()
      ? "expired"
      : "available";
  }
  return "unknown";
}

function isAvailableResetCredit(credit: CodexResetCredit): boolean {
  return resetCreditStatusKey(credit) === "available";
}

function resetCreditStatusLabel(credit: CodexResetCredit): string {
  const key = resetCreditStatusKey(credit);
  if (key === "available") return t("可用");
  if (key === "used") return t("已使用");
  if (key === "expired") return t("已过期");
  return credit.raw_status || credit.status || t("未知");
}

function formatResetCreditDate(value?: number): string {
  return Number.isFinite(value) ? formatTime(Number(value)) : t("时间未知");
}

function resetLogId(prefix: string): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return `${prefix}-${crypto.randomUUID()}`;
  }
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function enqueueResetStateOperation<T>(operation: () => Promise<T>): Promise<T> {
  const queued = resetStateOperationTail.then(operation, operation);
  resetStateOperationTail = queued.then(
    () => undefined,
    () => undefined,
  );
  return queued;
}

async function runResetStateMutation<T>(
  operation: () => Promise<T>,
  stateFromResult: (result: T) => ResetState,
  errorPrefix: string,
  silent = false,
): Promise<T> {
  resetStatePendingMutations += 1;
  resetStateSaving.value = true;
  try {
    const result = await enqueueResetStateOperation(operation);
    resetState.value = stateFromResult(result);
    return result;
  } catch (error) {
    if (!silent) Message.error(`${t(errorPrefix)}: ${errorText(error)}`);
    throw error;
  } finally {
    resetStatePendingMutations = Math.max(0, resetStatePendingMutations - 1);
    resetStateSaving.value = resetStatePendingMutations > 0;
  }
}

async function loadResetStateFrom(loader: () => Promise<ResetState>): Promise<void> {
  if (resetStateLoadPromise) return resetStateLoadPromise;
  resetStateLoading.value = true;
  resetStateLoadPromise = enqueueResetStateOperation(loader)
    .then((state) => {
      resetState.value = state;
    })
    .catch((error) => {
      Message.error(formatTranslatedText("加载重置记录失败：{error}", {
        error: errorText(error),
      }));
    })
    .finally(() => {
      resetStateLoading.value = false;
      resetStateLoadPromise = undefined;
    });
  return resetStateLoadPromise;
}

async function loadResetState(): Promise<void> {
  return loadResetStateFrom(getCodexResetState);
}

async function initializeResetState(): Promise<void> {
  return loadResetStateFrom(initializeCodexResetState);
}

function scheduledResetForAccount(accountId: string): ScheduledReset | null {
  return (
    resetState.value.scheduledResets.find(
      (task) =>
        task.accountId === accountId &&
        (task.status === "scheduled" || task.status === "running"),
    ) ?? null
  );
}

async function handleScheduleReset(payload: { scheduledAt: number }): Promise<boolean> {
  const account = resetCreditAccount.value;
  if (!account) return false;
  if (!Number.isFinite(payload.scheduledAt) || payload.scheduledAt <= Date.now()) {
    Message.warning("预约时间必须晚于当前时间");
    return false;
  }
  if (scheduledResetForAccount(account.id)) {
    Message.warning("该账号已有预约重置");
    return false;
  }
  if (!hasAvailableResetCreditForModal.value) {
    Message.warning("当前账号没有可用的重置次数");
    return false;
  }
  const now = Date.now();
  const task: ScheduledReset = {
    id: resetLogId("schedule"),
    accountId: account.id,
    accountLabel: displayNameForUi(account),
    scheduledAt: Math.floor(payload.scheduledAt / 60_000) * 60_000,
    status: "scheduled",
    createdAt: now,
  };
  try {
    await runResetStateMutation(
      () => createCodexScheduledReset(task),
      (state) => state,
      "保存预约重置失败",
    );
    Message.success("预约重置已保存");
    return true;
  } catch {
    // runResetStateMutation 已经显示具体错误。
    return false;
  }
}

function handleViewResetSchedules(): void {
  updateResetScheduleVisible(false);
  resetCreditVisible.value = false;
  switchView("resets");
}

function handleOpenResetSchedule(): void {
  const account = resetCreditAccount.value;
  if (!account) return;
  if (resolveResetScheduleEntry(Boolean(scheduledResetForAccount(account.id))) === "view") {
    handleViewResetSchedules();
    return;
  }
  if (!hasAvailableResetCreditForModal.value) {
    Message.warning("当前账号没有可用的重置次数");
    return;
  }
  editingResetSchedule.value = null;
  resetScheduleVisible.value = true;
}

function handleEditScheduledReset(task: ScheduledReset): void {
  const current = resetState.value.scheduledResets.find((item) => item.id === task.id);
  if (!current || current.status !== "scheduled" || current.scheduledAt <= Date.now()) {
    Message.warning("预约已开始执行或已无法修改");
    return;
  }
  editingResetSchedule.value = current;
  resetScheduleVisible.value = true;
}

function updateResetScheduleVisible(visible: boolean): void {
  resetScheduleVisible.value = visible;
  if (!visible) editingResetSchedule.value = null;
}

async function handleUpdateScheduledReset(
  scheduleId: string,
  scheduledAt: number,
): Promise<boolean> {
  if (!Number.isFinite(scheduledAt) || scheduledAt <= Date.now()) {
    Message.warning("预约时间必须晚于当前时间");
    return false;
  }
  const task = resetState.value.scheduledResets.find((item) => item.id === scheduleId);
  if (!task || task.status !== "scheduled" || task.scheduledAt <= Date.now()) {
    Message.warning("预约已开始执行或已无法修改");
    return false;
  }
  const nextUpdatingIds = beginPendingItem(updatingResetScheduleIds.value, scheduleId);
  if (!nextUpdatingIds) return false;
  updatingResetScheduleIds.value = nextUpdatingIds;
  try {
    await runResetStateMutation(
      () => updateCodexScheduledReset(scheduleId, Math.floor(scheduledAt / 60_000) * 60_000),
      (state) => state,
      "保存预约修改失败",
    );
    Message.success("预约时间已更新");
    return true;
  } catch {
    // runResetStateMutation 已经显示具体错误。
    return false;
  } finally {
    updatingResetScheduleIds.value = finishPendingItem(
      updatingResetScheduleIds.value,
      scheduleId,
    );
  }
}

async function handleSaveResetSchedule(scheduledAt: number): Promise<void> {
  if (resetStateSaving.value) return;
  const editingTask = editingResetSchedule.value;
  if (editingTask) {
    const updated = await handleUpdateScheduledReset(editingTask.id, scheduledAt);
    if (!updated) return;
    updateResetScheduleVisible(false);
    return;
  }
  const created = await handleScheduleReset({ scheduledAt });
  if (!created) return;
  updateResetScheduleVisible(false);
  resetCreditVisible.value = false;
}

async function handleCancelScheduledReset(scheduleId: string): Promise<void> {
  const task = resetState.value.scheduledResets.find((item) => item.id === scheduleId);
  if (!task || task.status !== "scheduled") return;
  const nextCancellingIds = beginPendingItem(
    cancellingResetScheduleIds.value,
    scheduleId,
  );
  if (!nextCancellingIds) return;
  cancellingResetScheduleIds.value = nextCancellingIds;
  const now = Date.now();
  try {
    await runResetStateMutation(
      () => cancelCodexScheduledReset(scheduleId, now, resetLogId("log")),
      (state) => state,
      "取消预约失败",
    );
    Message.success("预约已取消");
  } catch {
    // runResetStateMutation 已经显示具体错误。
  } finally {
    cancellingResetScheduleIds.value = finishPendingItem(
      cancellingResetScheduleIds.value,
      scheduleId,
    );
  }
}

async function handleDeleteResetLog(logId: string): Promise<void> {
  if (clearingResetLogs.value || !resetState.value.logs.some((log) => log.id === logId)) return;
  const nextDeletingIds = beginPendingItem(deletingResetLogIds.value, logId);
  if (!nextDeletingIds) return;
  deletingResetLogIds.value = nextDeletingIds;
  try {
    await runResetStateMutation(
      () => deleteCodexResetLog(logId),
      (state) => state,
      "删除重置日志失败",
    );
    Message.success("重置日志已删除");
  } catch {
    // runResetStateMutation 已经显示具体错误。
  } finally {
    deletingResetLogIds.value = finishPendingItem(deletingResetLogIds.value, logId);
  }
}

async function handleClearResetLogs(): Promise<void> {
  if (
    clearingResetLogs.value ||
    deletingResetLogIds.value.length > 0 ||
    resetState.value.logs.length === 0
  ) {
    return;
  }
  clearingResetLogs.value = true;
  try {
    await runResetStateMutation(
      clearCodexResetLogs,
      (state) => state,
      "清空重置日志失败",
    );
    Message.success("重置日志已清空");
  } catch {
    // runResetStateMutation 已经显示具体错误。
  } finally {
    clearingResetLogs.value = false;
  }
}

async function executeScheduledReset(task: ScheduledReset): Promise<void> {
  let resetError: unknown;
  let quotaRefreshError: string | undefined;
  try {
    const result = await consumeCodexResetCredit(task.accountId);
    quotaRefreshError = result.quotaRefreshError;
  } catch (error) {
    resetError = error;
  }
  const finishedAt = Date.now();
  if (resetError) {
    let logError: unknown;
    try {
      await runResetStateMutation(
        () =>
          finishCodexScheduledReset(
            task.id,
            finishedAt,
            "failed",
            errorText(resetError),
            resetLogId("log"),
          ),
        (state) => state,
        "保存预约失败日志失败",
        true,
      );
    } catch (error) {
      logError = error;
    }
    const suffix = logError
      ? formatTranslatedText("；保存日志失败：{error}", { error: errorText(logError) })
      : "";
    Message.error(`${formatTranslatedText("{account} 预约重置失败：{error}", {
      account: task.accountLabel,
      error: errorText(resetError),
    })}${suffix}`);
    return;
  }

  let logError: unknown;
  try {
    await runResetStateMutation(
      () =>
        finishCodexScheduledReset(
          task.id,
          finishedAt,
          "success",
          undefined,
          resetLogId("log"),
        ),
      (state) => state,
      "保存预约成功日志失败",
      true,
    );
  } catch (error) {
    logError = error;
  }
  await loadAccounts();
  if (logError) {
    Message.error(formatTranslatedText("{account} 已重置，但保存日志失败：{error}", {
      account: task.accountLabel,
      error: errorText(logError),
    }));
  } else if (quotaRefreshError) {
    Message.warning(formatTranslatedText("{account} 预约重置完成，但刷新额度失败：{error}", {
      account: task.accountLabel,
      error: quotaRefreshError,
    }));
  } else {
    Message.success(formatTranslatedText("{account} 预约重置完成", {
      account: task.accountLabel,
    }));
  }
}

async function runDueResetTasks(): Promise<void> {
  if (resetExecutionPromise) return;
  const hasDueTask = resetState.value.scheduledResets.some(
    (task) => task.status === "scheduled" && task.scheduledAt <= Date.now(),
  );
  if (!hasDueTask) return;
  resetExecutionPromise = (async () => {
    while (true) {
      const task = resetState.value.scheduledResets
        .filter((item) => item.status === "scheduled" && item.scheduledAt <= Date.now())
        .sort((left, right) => left.scheduledAt - right.scheduledAt)[0];
      if (!task) return;
      const claim = await runResetStateMutation(
        () => claimCodexScheduledReset(task.id),
        (result) => result.state,
        "领取预约任务失败",
        true,
      );
      if (!claim.task) continue;
      await executeScheduledReset(claim.task);
    }
  })().catch((error) => {
    Message.error(formatTranslatedText("执行预约任务失败：{error}", {
      error: errorText(error),
    }));
  }).finally(() => {
    resetExecutionPromise = undefined;
  });
  await resetExecutionPromise;
}

function isFreePlanAccount(account: CodexAccount): boolean {
  return !isApiKeyAccount(account) && effectivePlanKey(account) === "free";
}

function errorText(error: unknown): string {
  return String(error instanceof Error ? error.message : error).replace(/^Error:\s*/, "");
}

async function drainAccountLoadQueue(): Promise<void> {
  loading.value = true;
  try {
    while (accountLoadRequested) {
      accountLoadRequested = false;
      try {
        const [nextAccounts, nextCurrent] = await Promise.all([
          listCodexAccounts(),
          getCurrentCodexAccount(),
        ]);
        let shouldPrefetchBalances = false;
        const previousAccounts = new Map(accounts.value.map((account) => [account.id, account]));
        const nextAccountIds = new Set(nextAccounts.map((account) => account.id));
        for (const account of nextAccounts) {
          const previous = previousAccounts.get(account.id);
          if (previous && apiKeyCredentialsChanged(previous, account)) {
            invalidateApiKeyBalance(account.id);
            shouldPrefetchBalances = true;
          }
        }
        const trackedBalanceAccountIds = new Set([
          ...Object.keys(apiKeyBalanceStates),
          ...apiKeyBalanceRequestSequences.keys(),
        ]);
        for (const accountId of trackedBalanceAccountIds) {
          if (!nextAccountIds.has(accountId)) forgetApiKeyBalance(accountId);
        }
        accounts.value = nextAccounts;
        currentAccount.value = nextCurrent;
        if (shouldPrefetchBalances && activeView.value === "accounts") {
          void nextTick(() => prefetchVisibleApiKeyBalances());
        }
      } catch (error) {
        Message.error(`加载账号失败：${errorText(error)}`);
      }
    }
  } finally {
    loading.value = false;
  }
}

function loadAccounts(): Promise<void> {
  accountLoadRequested = true;
  if (!accountLoadPromise) {
    accountLoadPromise = drainAccountLoadQueue().finally(() => {
      accountLoadPromise = undefined;
    });
  }
  return accountLoadPromise;
}

function apiKeyCredentialsChanged(previous: CodexAccount, next: CodexAccount): boolean {
  return (
    (previous.openai_api_key || previous.openaiApiKey || "") !==
      (next.openai_api_key || next.openaiApiKey || "") ||
    (previous.api_base_url || previous.apiBaseUrl || "") !==
      (next.api_base_url || next.apiBaseUrl || "")
  );
}

function invalidateApiKeyBalance(accountId: string): void {
  apiKeyBalanceRequestSequences.set(accountId, ++apiKeyBalanceRequestSequence);
  apiKeyBalanceLastAttemptAt.delete(accountId);
  apiKeyBalanceInsecureHttpApprovals.delete(accountId);
  delete apiKeyBalanceStates[accountId];
}

function forgetApiKeyBalance(accountId: string): void {
  invalidateApiKeyBalance(accountId);
  apiKeyBalanceInsecureHttpConfirmationPending.delete(accountId);
  apiKeyBalanceRequestSequences.delete(accountId);
}

async function loadApiKeyBalance(
  account: CodexAccount,
  options: { force?: boolean; silent?: boolean } = {},
): Promise<void> {
  if (!isApiKeyAccount(account)) return;
  if (!apiKeyBalanceCanAutoFetch(account)) return;
  const previous = apiKeyBalanceStates[account.id];
  if (previous?.status === "loading") return;
  const lastAttemptAt = apiKeyBalanceLastAttemptAt.get(account.id) || 0;
  if (!options.force && lastAttemptAt > 0 && Date.now() - lastAttemptAt < API_KEY_BALANCE_TTL_MS) {
    return;
  }

  const requestSequence = ++apiKeyBalanceRequestSequence;
  apiKeyBalanceRequestSequences.set(account.id, requestSequence);
  apiKeyBalanceStates[account.id] = {
    status: "loading",
    balance: previous?.balance,
    fetchedAt: previous?.fetchedAt || 0,
  };
  try {
    const insecureHttpOrigin = remoteHttpBalanceOrigin(account);
    const approvedInsecureHttpOrigin =
      insecureHttpOrigin && apiKeyBalanceInsecureHttpApprovals.get(account.id) === insecureHttpOrigin
        ? insecureHttpOrigin
        : undefined;
    const balance = await fetchCodexApiKeyBalance(account.id, approvedInsecureHttpOrigin);
    if (apiKeyBalanceRequestSequences.get(account.id) !== requestSequence) return;
    apiKeyBalanceStates[account.id] = {
      status: "success",
      balance,
      fetchedAt: Date.now(),
    };
    apiKeyBalanceLastAttemptAt.set(account.id, Date.now());
    if (!options.silent) Message.success(t("余额已刷新"));
  } catch (error) {
    if (apiKeyBalanceRequestSequences.get(account.id) !== requestSequence) return;
    const message = errorText(error);
    if (message.startsWith(INSECURE_HTTP_CONFIRM_REQUIRED)) {
      apiKeyBalanceStates[account.id] = {
        status: "consent_required",
        balance: previous?.balance,
        fetchedAt: previous?.fetchedAt || 0,
      };
      apiKeyBalanceLastAttemptAt.delete(account.id);
      return;
    }
    apiKeyBalanceStates[account.id] = {
      status: "error",
      balance: previous?.balance,
      error: message,
      fetchedAt: previous?.fetchedAt || 0,
    };
    apiKeyBalanceLastAttemptAt.set(account.id, Date.now());
    if (!options.silent) Message.warning(`${t("余额获取失败")}：${message}`);
  }
}

function runApiKeyBalancePrefetchWorkers(): void {
  while (
    apiKeyBalancePrefetchWorkers < API_KEY_BALANCE_PREFETCH_CONCURRENCY &&
    apiKeyBalancePrefetchQueue.length > 0
  ) {
    apiKeyBalancePrefetchWorkers += 1;
    void (async () => {
      try {
        while (apiKeyBalancePrefetchQueue.length > 0) {
          const task = apiKeyBalancePrefetchQueue.shift();
          if (
            !task ||
            task.generation !== apiKeyBalancePrefetchGeneration ||
            activeView.value !== "accounts"
          ) {
            continue;
          }
          await loadApiKeyBalance(task.account, { silent: true });
        }
      } finally {
        apiKeyBalancePrefetchWorkers -= 1;
        runApiKeyBalancePrefetchWorkers();
      }
    })();
  }
}

function cancelApiKeyBalancePrefetch(): void {
  apiKeyBalancePrefetchGeneration += 1;
  apiKeyBalancePrefetchQueue.splice(0);
}

function prefetchVisibleApiKeyBalances(): void {
  cancelApiKeyBalancePrefetch();
  if (activeView.value !== "accounts") return;
  const generation = apiKeyBalancePrefetchGeneration;
  for (const account of pagedAccounts.value) {
    if (isApiKeyAccount(account) && apiKeyBalanceCanAutoFetch(account)) {
      apiKeyBalancePrefetchQueue.push({ account, generation });
    }
  }
  runApiKeyBalancePrefetchWorkers();
}

async function handleRefreshApiKeyBalance(account: CodexAccount): Promise<void> {
  const insecureHttpOrigin = remoteHttpBalanceOrigin(account);
  if (
    insecureHttpOrigin &&
    apiKeyBalanceInsecureHttpApprovals.get(account.id) !== insecureHttpOrigin
  ) {
    if (apiKeyBalanceInsecureHttpConfirmationPending.has(account.id)) return;
    apiKeyBalanceInsecureHttpConfirmationPending.add(account.id);
    Modal.warning({
      title: t("HTTP 连接安全提示"),
      content: `${t("该中转站使用未加密的 HTTP 连接。查询余额时会通过此连接发送 API Key，同一网络中的设备或代理可能读取它。是否仅在本次运行期间允许？")}\n${insecureHttpOrigin}`,
      okText: t("仅本次运行允许"),
      cancelText: t("取消"),
      hideCancel: false,
      async onOk() {
        try {
          const current = accounts.value.find((item) => item.id === account.id);
          if (
            !current ||
            remoteHttpBalanceOrigin(current) !== insecureHttpOrigin ||
            apiKeyCredentialsChanged(account, current)
          ) {
            Message.warning(t("账号配置已变化，请重新点击余额并确认"));
            return;
          }
          apiKeyBalanceInsecureHttpApprovals.set(account.id, insecureHttpOrigin);
          await loadApiKeyBalance(current, { force: true, silent: false });
        } finally {
          apiKeyBalanceInsecureHttpConfirmationPending.delete(account.id);
        }
      },
      onCancel() {
        apiKeyBalanceInsecureHttpConfirmationPending.delete(account.id);
      },
    });
    return;
  }
  await loadApiKeyBalance(account, { force: true, silent: false });
}

async function handleDetectCurrentAccount(): Promise<void> {
  detectingCurrentAccount.value = true;
  try {
    const detected = await detectCurrentCodexAccount();
    if (!detected) {
      currentAccount.value = null;
      Message.warning("未能从当前 Codex 配置匹配到账号");
      return;
    }
    currentAccount.value = detected;
    nextCurrentAccountRefreshAt.value = 0;
    resetCurrentAccountQuotaTimer();
    Message.success(`已读取当前账号：${displayNameForUi(detected)}`);
  } catch (error) {
    Message.error(`读取当前账号失败：${errorText(error)}`);
  } finally {
    detectingCurrentAccount.value = false;
  }
}

async function loadSettings(options: { includeStorage?: boolean } = {}): Promise<void> {
  settingsLoading.value = true;
  try {
    const [nextSettings, nextPaths, nextBackups] = await Promise.all([
      getCodexSwitcherSettings(),
      options.includeStorage === false ? Promise.resolve(null) : getCodexSwitcherPaths(),
      options.includeStorage === false ? Promise.resolve(null) : listCodexSwitcherBackups(),
    ]);
    Object.assign(settings, {
      ...nextSettings,
      monitorQuota: true,
      badgeStyles: {
        ...defaultBadgeStyles(),
        ...(nextSettings.badgeStyles || {}),
      },
      sortDirection: nextSettings.sortDirection === "asc" ? "asc" : "desc",
      pinnedAccountIds: nextSettings.pinnedAccountIds || [],
      accountTypeFilter: nextSettings.accountTypeFilter || "all",
      pageSize: Math.max(1, Number(nextSettings.pageSize || 50)),
      accountViewMode: normalizeAccountViewMode(nextSettings.accountViewMode),
      sidebarEnabled: nextSettings.sidebarEnabled ?? true,
      quotaRefreshMinutes: clampRefreshMinutes(nextSettings.quotaRefreshMinutes),
      currentAccountRefreshMinutes: clampRefreshMinutes(nextSettings.currentAccountRefreshMinutes),
      quotaNextRefreshAt: Number(nextSettings.quotaNextRefreshAt || 0),
      currentAccountNextRefreshAt: Number(nextSettings.currentAccountNextRefreshAt || 0),
      showQuotaCountdowns: nextSettings.showQuotaCountdowns ?? true,
      showAdditionalQuotaWindows: nextSettings.showAdditionalQuotaWindows ?? true,
      maxColumns: [3, 4, 5].includes(nextSettings.maxColumns) ? nextSettings.maxColumns : 5,
      language: nextSettings.language || "zh-CN",
    });
    setLanguage(settings.language);
    if (nextPaths) appPaths.value = nextPaths;
    if (nextBackups) backupFiles.value = nextBackups;
    resetQuotaTimer();
    resetCurrentAccountQuotaTimer();
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
      monitorQuota: true,
      badgeStyles: {
        ...defaultBadgeStyles(),
        ...settings.badgeStyles,
      },
      sortDirection: settings.sortDirection === "asc" ? "asc" : "desc",
      pinnedAccountIds: settings.pinnedAccountIds || [],
      accountTypeFilter: settings.accountTypeFilter || "all",
      pageSize: Math.max(1, Number(settings.pageSize || 50)),
      accountViewMode: normalizeAccountViewMode(settings.accountViewMode),
      sidebarEnabled: settings.sidebarEnabled ?? true,
      quotaRefreshMinutes: clampRefreshMinutes(settings.quotaRefreshMinutes),
      currentAccountRefreshMinutes: clampRefreshMinutes(settings.currentAccountRefreshMinutes),
      quotaNextRefreshAt: settings.monitorQuota ? Math.floor(settings.quotaNextRefreshAt || 0) : 0,
      currentAccountNextRefreshAt: settings.monitorQuota
        ? Math.floor(settings.currentAccountNextRefreshAt || 0)
        : 0,
      showQuotaCountdowns: settings.showQuotaCountdowns ?? true,
      showAdditionalQuotaWindows: settings.showAdditionalQuotaWindows ?? true,
      maxColumns: [3, 4, 5].includes(settings.maxColumns) ? settings.maxColumns : 5,
      language: settings.language || "zh-CN",
    });
    Object.assign(settings, saved);
    setLanguage(settings.language);
    resetQuotaTimer();
    resetCurrentAccountQuotaTimer();
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
  if (!settings.monitorQuota || !currentAccount.value) return;
  const minutes = clampRefreshMinutes(settings.currentAccountRefreshMinutes);
  const nextAt = forceNew
    ? Date.now() + minutes * 60_000
    : normalizedNextRefreshAt(storedNextAt, minutes);
  scheduleCurrentAccountQuotaTimer(nextAt);
}

function resetQuotaTimer(forceNew = false): void {
  if (quotaTimer) {
    window.clearTimeout(quotaTimer);
    quotaTimer = undefined;
  }
  const storedNextAt = settings.quotaNextRefreshAt;
  setQuotaNextRefreshAt(0, false);
  if (!settings.monitorQuota) {
    return;
  }
  const minutes = clampRefreshMinutes(settings.quotaRefreshMinutes);
  const nextAt = forceNew
    ? Date.now() + minutes * 60_000
    : normalizedNextRefreshAt(storedNextAt, minutes);
  scheduleQuotaTimer(nextAt);
}

function scheduleCurrentAccountQuotaTimer(nextAt: number): void {
  if (currentAccountQuotaTimer) {
    window.clearTimeout(currentAccountQuotaTimer);
    currentAccountQuotaTimer = undefined;
  }
  setCurrentAccountNextRefreshAt(nextAt);
  currentAccountQuotaTimer = window.setTimeout(() => {
    void runScheduledCurrentAccountQuotaRefresh();
  }, Math.max(1_000, nextAt - Date.now()));
}

function scheduleQuotaTimer(nextAt: number): void {
  if (quotaTimer) {
    window.clearTimeout(quotaTimer);
    quotaTimer = undefined;
  }
  setQuotaNextRefreshAt(nextAt);
  quotaTimer = window.setTimeout(() => {
    void runScheduledQuotaRefresh();
  }, Math.max(1_000, nextAt - Date.now()));
}

function scheduleNextCurrentAccountQuotaCycle(): void {
  if (!settings.monitorQuota || !currentAccount.value) {
    resetCurrentAccountQuotaTimer();
    return;
  }
  scheduleCurrentAccountQuotaTimer(
    Date.now() + clampRefreshMinutes(settings.currentAccountRefreshMinutes) * 60_000,
  );
}

function scheduleNextQuotaCycle(): void {
  if (!settings.monitorQuota) {
    resetQuotaTimer();
    return;
  }
  scheduleQuotaTimer(Date.now() + clampRefreshMinutes(settings.quotaRefreshMinutes) * 60_000);
}

async function runScheduledCurrentAccountQuotaRefresh(): Promise<void> {
  if (scheduledCurrentAccountRefreshRunning) return;
  scheduledCurrentAccountRefreshRunning = true;
  try {
    await handleRefreshCurrentQuota(false, false);
  } finally {
    scheduledCurrentAccountRefreshRunning = false;
    scheduleNextCurrentAccountQuotaCycle();
  }
}

async function runScheduledQuotaRefresh(): Promise<void> {
  if (scheduledQuotaRefreshRunning) return;
  scheduledQuotaRefreshRunning = true;
  try {
    await handleRefreshAllQuotas(false, false);
  } finally {
    scheduledQuotaRefreshRunning = false;
    scheduleNextQuotaCycle();
  }
}

function refreshOverdueQuotaCountdowns(): void {
  if (!settings.monitorQuota) return;
  const now = Date.now();
  if (
    nextCurrentAccountRefreshAt.value > 0 &&
    now >= nextCurrentAccountRefreshAt.value &&
    !scheduledCurrentAccountRefreshRunning
  ) {
    void runScheduledCurrentAccountQuotaRefresh();
  }
  if (
    nextQuotaRefreshAt.value > 0 &&
    now >= nextQuotaRefreshAt.value &&
    !scheduledQuotaRefreshRunning
  ) {
    void runScheduledQuotaRefresh();
  }
}

async function handleRefreshCurrentQuota(
  showMessage = true,
  updateNextRefresh = true,
): Promise<void> {
  if (!settings.monitorQuota || !currentAccount.value) {
    return;
  }
  const current = currentAccount.value;
  const targetIds = await apiServiceQuotaRefreshAccountIds();
  if (canShowQuota(current)) targetIds.add(current.id);
  const targets = [...targetIds]
    .map((id) => accounts.value.find((account) => account.id === id) ?? (
      current.id === id ? current : undefined
    ))
    .filter((account): account is CodexAccount => Boolean(account))
    .filter(canShowQuota);
  if (!targets.length) return;
  let refreshedCount = 0;
  let failedCount = 0;
  try {
    for (const account of targets) {
      quotaRefreshingId.value = account.id;
      try {
        const updated = await refreshCodexQuota(account.id);
        refreshedCount += 1;
        if (currentAccount.value?.id === updated.id) {
          currentAccount.value = updated;
        }
      } catch {
        failedCount += 1;
      }
    }
    await loadAccounts();
    if (updateNextRefresh) {
      resetCurrentAccountQuotaTimer(true);
    }
    if (showMessage) {
      if (failedCount > 0) {
        Message.warning(`已刷新 ${refreshedCount} 个账号额度，${failedCount} 个失败`);
      } else {
        Message.success(refreshedCount > 1 ? `已刷新 ${refreshedCount} 个账号额度` : "当前账号额度已刷新");
      }
    }
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

async function refreshAccountsAfterMutation(changedAccounts: CodexAccount[]): Promise<number> {
  const uniqueAccounts = changedAccounts.filter(
    (account, index, items) => items.findIndex((item) => item.id === account.id) === index,
  );
  let quotaFailures = 0;
  try {
    for (const account of uniqueAccounts) {
      const latest = accounts.value.find((item) => item.id === account.id) ?? account;
      if (isApiKeyAccount(latest)) {
        invalidateApiKeyBalance(latest.id);
        await loadApiKeyBalance(latest, { force: true, silent: true });
        continue;
      }
      quotaRefreshingId.value = latest.id;
      try {
        const refreshed = await refreshCodexQuota(latest.id);
        accounts.value = accounts.value.map((item) =>
          item.id === refreshed.id ? refreshed : item,
        );
        if (currentAccount.value?.id === refreshed.id) {
          currentAccount.value = refreshed;
        }
      } catch {
        quotaFailures += 1;
      }
    }
  } finally {
    quotaRefreshingId.value = "";
  }
  if (quotaFailures > 0) await loadAccounts();
  return quotaFailures;
}

function warnQuotaRefreshFailures(count: number): void {
  if (count > 0) {
    Message.warning(`${count} 个账号已保存，但额度刷新失败，可稍后点击刷新重试`);
  }
}

async function handleApiServiceAccountAdded(account: CodexAccount): Promise<void> {
  await loadAccounts();
  await refreshApiServiceAccountIds();
  const quotaFailures = await refreshAccountsAfterMutation([account]);
  warnQuotaRefreshFailures(quotaFailures);
}

async function refreshApiServiceAccountIds(showError = false): Promise<void> {
  try {
    const bound = await listApiServiceBoundAccounts();
    apiServiceAccountIds.value = new Set(
      bound
        .map((account) => account.accountId)
        .filter((accountId): accountId is string => Boolean(accountId)),
    );
  } catch (error) {
    apiServiceAccountIds.value = new Set();
    if (showError) Message.error(`读取 API 服务账号失败：${errorText(error)}`);
  }
}

async function apiServiceQuotaRefreshAccountIds(): Promise<Set<string>> {
  try {
    const serviceState = await getApiServiceState();
    if (!serviceState.service.running) return new Set();
    await refreshApiServiceAccountIds();
    return new Set(apiServiceAccountIds.value);
  } catch {
    return new Set();
  }
}

async function confirmResetCredit(account: CodexAccount): Promise<void> {
  let targetAccount = account;
  let count = resetCreditCount(targetAccount);
  if (count <= 0) {
    Message.warning("当前账号没有可用的重置次数");
    return;
  }

  if (!resetCreditRecords(targetAccount).length) {
    quotaRefreshingId.value = targetAccount.id;
    try {
      const updated = await refreshCodexQuota(targetAccount.id);
      await loadAccounts();
      targetAccount = accounts.value.find((item) => item.id === updated.id) ?? updated;
      count = resetCreditCount(targetAccount);
    } catch (error) {
      await loadAccounts();
      Message.warning(formatTranslatedText("获取重置次数明细失败：{error}", {
        error: errorText(error),
      }));
      return;
    } finally {
      quotaRefreshingId.value = "";
    }
  }

  if (count <= 0) {
    Message.warning("当前账号没有可用的重置次数");
    return;
  }
  if (!resetCreditRecords(targetAccount).length) {
    Message.warning("未获取到重置次数明细，请先刷新额度后重试");
    return;
  }

  resetCreditAccount.value = targetAccount;
  resetCreditVisible.value = true;
}

async function handleConsumeSelectedResetCredit(): Promise<void> {
  const account = resetCreditAccount.value;
  if (!account) return;
  if (scheduledResetForAccount(account.id)) {
    Message.warning("该账号已有活动预约，请先取消预约");
    return;
  }
  if (!hasAvailableResetCreditForModal.value) {
    Message.warning("当前账号没有可用的重置次数");
    return;
  }
  quotaRefreshingId.value = account.id;
  try {
    const occurredAt = Date.now();
    let resetError: unknown;
    let quotaRefreshError: string | undefined;
    try {
      const result = await consumeCodexResetCredit(account.id);
      quotaRefreshError = result.quotaRefreshError;
    } catch (error) {
      resetError = error;
    }
    if (resetError) {
      let logError: unknown;
      try {
        await runResetStateMutation(
          () =>
            appendCodexResetLog({
              id: resetLogId("log"),
              accountId: account.id,
              accountLabel: displayNameForUi(account),
              type: "immediate",
              occurredAt,
              result: "failed",
              error: errorText(resetError),
            }),
          (state) => state,
          "保存立即重置失败日志失败",
          true,
        );
      } catch (error) {
        logError = error;
      }
      await loadAccounts();
      const suffix = logError
        ? formatTranslatedText("；保存日志失败：{error}", { error: errorText(logError) })
        : "";
      Message.error(`${formatTranslatedText("重置额度失败：{error}", {
        error: errorText(resetError),
      })}${suffix}`);
    } else {
      let logError: unknown;
      try {
        await runResetStateMutation(
          () =>
            appendCodexResetLog({
              id: resetLogId("log"),
              accountId: account.id,
              accountLabel: displayNameForUi(account),
              type: "immediate",
              occurredAt,
              result: "success",
            }),
          (state) => state,
          "保存立即重置成功日志失败",
          true,
        );
      } catch (error) {
        logError = error;
      }
      resetCreditVisible.value = false;
      resetCreditAccount.value = null;
      await loadAccounts();
      if (logError) {
        Message.error(formatTranslatedText("额度已重置，但保存日志失败：{error}", {
          error: errorText(logError),
        }));
      } else if (quotaRefreshError) {
        Message.warning(formatTranslatedText("额度已重置，但刷新额度失败：{error}", {
          error: quotaRefreshError,
        }));
      } else {
        Message.success("额度已重置");
      }
    }
  } finally {
    quotaRefreshingId.value = "";
  }
}

async function handleRefreshAllQuotas(
  showMessage = true,
  updateNextRefresh = true,
): Promise<void> {
  if (!settings.monitorQuota) return;
  quotaRefreshingId.value = "__all__";
  let refreshedCount = 0;
  let failedCount = 0;
  try {
    const candidates = pagedAccounts.value.filter(canShowQuota);
    for (const account of candidates) {
      quotaRefreshingId.value = account.id;
      try {
        await refreshCodexQuota(account.id);
        refreshedCount += 1;
      } catch {
        failedCount += 1;
      }
    }
    await loadAccounts();
    if (settings.monitorQuota && updateNextRefresh) {
      resetQuotaTimer(true);
    }
    if (showMessage) {
      if (failedCount > 0) {
        Message.warning(`已刷新当前页 ${refreshedCount} 个账号额度，${failedCount} 个失败`);
      } else {
        Message.success(`已刷新当前页 ${refreshedCount} 个账号额度`);
      }
    }
  } catch (error) {
    await loadAccounts();
    if (showMessage) Message.warning(`批量刷新额度失败：${errorText(error)}`);
  } finally {
    quotaRefreshingId.value = "";
  }
}

async function handleRefreshEveryQuota(): Promise<void> {
  if (!settings.monitorQuota) {
    Message.warning("请先在设置中开启额度监控");
    return;
  }
  refreshingAllQuotas.value = true;
  quotaRefreshingId.value = "__all__";
  try {
    const count = await refreshAllCodexQuotas();
    await loadAccounts();
    resetQuotaTimer(true);
    Message.success(count > 0 ? `已刷新全部 ${count} 个账号额度` : "没有可刷新的 OAuth 账号");
  } catch (error) {
    await loadAccounts();
    Message.warning(`刷新全部额度失败：${errorText(error)}`);
  } finally {
    refreshingAllQuotas.value = false;
    quotaRefreshingId.value = "";
  }
}

async function handleSwitch(account: CodexAccount): Promise<void> {
  if (switchingId.value) return;
  switchingId.value = account.id;
  try {
    currentAccount.value = await switchCodexAccount(account.id);
    let restartMessage = "已请求重启 ChatGPT/Codex";
    try {
      restartMessage = await restartCodexApp();
    } catch (restartError) {
      Message.warning(`账号已切换，但启动 ChatGPT/Codex 失败：${errorText(restartError)}`);
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
    cancelApiKeyBalancePrefetch();
    forgetApiKeyBalance(account.id);
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

function clearSelectedAccounts(): void {
  selectedAccountIds.value = new Set();
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

async function confirmBindSelectedToApiService(): Promise<void> {
  if (!selectedAccountIdList.value.length) {
    Message.warning("请先勾选要绑定到 API 服务的账号");
    return;
  }
  let serviceState;
  try {
    serviceState = await getApiServiceState();
  } catch (error) {
    Message.error(`读取 API 服务配置失败：${errorText(error)}`);
    return;
  }
  const selected = selectedAccountIdList.value
    .map((id) => accounts.value.find((account) => account.id === id))
    .filter((account): account is CodexAccount => Boolean(account))
    .filter((account) => !isCurrentApiServiceAccount(account, serviceState));
  if (!selected.length) {
    Message.warning("所选账号指向当前 API 服务，不能绑定服务自身");
    return;
  }
  const oauthCount = selected.filter((account) => !isApiKeyAccount(account)).length;
  const apiKeyCount = selected.length - oauthCount;
  Modal.warning({
    title: "绑定到 API 服务",
    content: `将先清空 API 服务中的现有账号，再写入本次选择的 ${selected.length} 个账号（OAuth ${oauthCount}，API Key ${apiKeyCount}）。是否继续？`,
    okText: "确认绑定",
    cancelText: "取消",
    hideCancel: false,
    async onOk() {
      try {
        const summary = await bindApiServiceAccounts(selected.map((account) => account.id));
        await refreshApiServiceAccountIds();
        Message.success(
          `已绑定 ${summary.count} 个账号到 API 服务（OAuth ${summary.oauthCount}，API Key ${summary.apiKeyCount}）`,
        );
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

function downloadJsonText(content: string, filename: string): void {
  let url = "";
  try {
    const blob = new Blob([content], { type: "application/json;charset=utf-8" });
    url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    link.click();
    Message.success(`已开始下载 ${filename}`);
  } catch (error) {
    Message.error(`下载失败：${errorText(error)}`);
  } finally {
    if (url) URL.revokeObjectURL(url);
  }
}

function downloadBatchExportText(): void {
  const suffix = exportFormat.value === "cockpit_tools" ? "" : `_${exportFormat.value}`;
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  downloadJsonText(batchExportText.value, `codex-switcher-batch${suffix}_${timestamp}.json`);
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
    const quotaFailures = await refreshAccountsAfterMutation(imported);
    Message.success(`成功导入 ${imported.length} 个账号`);
    warnQuotaRefreshFailures(quotaFailures);
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
    const quotaFailures = await refreshAccountsAfterMutation(imported);
    Message.success(`已从本机 Codex 导入 ${imported.length} 个账号`);
    warnQuotaRefreshFailures(quotaFailures);
  } catch (error) {
    Message.error(`本地导入失败：${errorText(error)}`);
  } finally {
    importing.value = false;
  }
}

async function handleFileImport(files: File[]): Promise<void> {
  if (!files.length) return;
  importing.value = true;
  try {
    const importedAccounts: CodexAccount[] = [];
    for (const file of files) {
      const content = await file.text();
      const imported = await importCodexFromJson(content);
      importedAccounts.push(...imported);
    }
    addModalVisible.value = false;
    await loadAccounts();
    const quotaFailures = await refreshAccountsAfterMutation(importedAccounts);
    Message.success(`已从 ${files.length} 个文件导入 ${importedAccounts.length} 个账号`);
    warnQuotaRefreshFailures(quotaFailures);
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
    const quotaFailures = await refreshAccountsAfterMutation([account]);
    Message.success(`已添加 ${displayName(account)}`);
    warnQuotaRefreshFailures(quotaFailures);
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

const authorProfileUrl = "https://github.com/vs2pk0";
const repositoryUrl = "https://github.com/vs2pk0/codex-switcher";
const sponsorUrl = "https://github.com/vs2pk0/codex-switcher/blob/main/doc/sponsor.md";
const feedbackUrl = "https://github.com/vs2pk0/codex-switcher/issues";
const releasesUrl = "https://github.com/vs2pk0/codex-switcher/releases";

function openAboutUrl(url: string, label: string): void {
  void openExternalUrl(url).catch((error) => {
    Message.error(`打开${label}失败：${errorText(error)}`);
  });
}

function openGithubProfile(): void {
  openAboutUrl(authorProfileUrl, "作者主页");
}

function openRepository(): void {
  openAboutUrl(repositoryUrl, "开源仓库");
}

function openSponsorPage(): void {
  openAboutUrl(sponsorUrl, "赞助页面");
}

function openFeedbackPage(): void {
  openAboutUrl(feedbackUrl, "问题反馈");
}

function openReleasesPage(): void {
  openAboutUrl(releasesUrl, "下载页面");
}

function showUpdateDialog(info: AppUpdateInfo): void {
  const content = info.canDownload
    ? `${t("当前版本")} v${info.currentVersion}，${t("最新版本")} v${info.latestVersion}。${t("可直接在应用内下载安装包。")}`
    : `${t("当前版本")} v${info.currentVersion}，${t("最新版本")} v${info.latestVersion}。${t("当前平台没有可用的在线安装包，请前往 GitHub Releases 下载。")}`;
  Modal.confirm({
    title: `${t("发现新版本")} v${info.latestVersion}`,
    content,
    okText: t(info.canDownload ? "在线更新" : "前往下载"),
    cancelText: t("稍后再说"),
    onOk: () => {
      if (info.canDownload) {
        void startAppUpdateDownload(info);
      } else {
        openAboutUrl(info.releaseUrl, "下载页面");
      }
    },
  });
}

async function checkAppUpdate(options: { silent?: boolean } = {}): Promise<void> {
  if (checkingAppUpdate.value) return;
  checkingAppUpdate.value = true;
  try {
    const info = await fetchAppUpdateInfo();
    appUpdateInfo.value = info;
    if (info.hasUpdate) {
      showUpdateDialog(info);
    } else if (!options.silent) {
      Message.success(`${t("当前已是最新版本")} v${info.currentVersion}`);
    }
  } catch (error) {
    if (!options.silent) {
      Modal.confirm({
        title: t("检查更新失败"),
        content: `${t("暂时无法获取最新版本信息")}：${errorText(error)}。${t("可以前往 GitHub Releases 手动查看。")}`,
        okText: t("打开 Releases"),
        cancelText: t("关闭"),
        onOk: openReleasesPage,
      });
    }
  } finally {
    checkingAppUpdate.value = false;
  }
}

async function startAppUpdateDownload(info: AppUpdateInfo | null = appUpdateInfo.value): Promise<void> {
  if (!info || appUpdateDownloading.value) return;
  appUpdateInfo.value = info;
  appUpdateProgress.value = {
    status: "checking",
    version: info.latestVersion,
    assetName: info.assetName || "",
    downloadedBytes: 0,
    totalBytes: info.assetSize || null,
    message: null,
  };
  appUpdateResult.value = null;
  appUpdateError.value = "";
  appUpdateVisible.value = true;
  appUpdateDownloading.value = true;
  try {
    appUpdateResult.value = await downloadAppUpdate();
  } catch (error) {
    const message = errorText(error);
    if (message !== "下载已取消") {
      appUpdateError.value = message;
    }
  } finally {
    appUpdateDownloading.value = false;
    appUpdateCancelling.value = false;
  }
}

async function cancelCurrentAppUpdate(): Promise<void> {
  if (!appUpdateDownloading.value || appUpdateCancelling.value) return;
  appUpdateCancelling.value = true;
  try {
    await cancelAppUpdateDownload();
  } catch (error) {
    appUpdateCancelling.value = false;
    Message.error(`${t("取消下载失败")}：${errorText(error)}`);
  }
}

async function openDownloadedAppUpdate(): Promise<void> {
  const result = appUpdateResult.value;
  if (!result || appUpdateOpening.value) return;
  appUpdateOpening.value = true;
  try {
    await openAppUpdateInstaller(result.path);
    Message.success(t("安装包已打开，请按安装程序完成更新。"));
  } catch (error) {
    Message.error(`${t("打开安装包失败")}：${errorText(error)}`);
  } finally {
    appUpdateOpening.value = false;
  }
}

function closeAppUpdateModal(): void {
  if (appUpdateDownloading.value) return;
  appUpdateVisible.value = false;
}

function openAppUpdateReleases(): void {
  openAboutUrl(appUpdateInfo.value?.releaseUrl || releasesUrl, "下载页面");
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
    const quotaFailures = await refreshAccountsAfterMutation([account]);
    Message.success(`已添加 ${displayName(account)}`);
    warnQuotaRefreshFailures(quotaFailures);
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
  editForm.tags = accountTags(account);
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
        tags: editForm.tags,
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
    const quotaFailures = await refreshAccountsAfterMutation([updated]);
    Message.success(`已更新 ${displayName(updated)}`);
    warnQuotaRefreshFailures(quotaFailures);
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
  const suffix = exportFormat.value === "cockpit_tools" ? "" : `_${exportFormat.value}`;
  downloadJsonText(exportText.value, `${name}${suffix}.json`);
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

function openApiModels(account: CodexAccount): void {
  if (!isApiKeyAccount(account)) return;
  apiModelRequestSequence += 1;
  apiModelSaveSequence += 1;
  apiModelAccount.value = account;
  apiModels.value = [];
  selectedApiModel.value = account.default_model || account.defaultModel || "";
  fetchingApiModels.value = false;
  savingApiModel.value = false;
  apiModelAccessStatus.value = "checking";
  apiModelAccessError.value = "";
  apiModelVisible.value = true;
  void checkApiModelAccess();
}

function updateApiModelVisible(visible: boolean): void {
  apiModelVisible.value = visible;
  if (visible) return;
  apiModelAccessSequence += 1;
  apiModelRequestSequence += 1;
  apiModelSaveSequence += 1;
  apiModelAccessStatus.value = "idle";
  apiModelAccessError.value = "";
  fetchingApiModels.value = false;
  savingApiModel.value = false;
}

async function checkApiModelAccess(): Promise<boolean> {
  const account = apiModelAccount.value;
  if (!account || !apiModelVisible.value) return false;
  const accountId = account.id;
  const accessSequence = ++apiModelAccessSequence;
  apiModelAccessStatus.value = "checking";
  apiModelAccessError.value = "";
  try {
    const allowed = await checkCodexApiKeyModelAccess(accountId);
    if (
      accessSequence !== apiModelAccessSequence ||
      apiModelAccount.value?.id !== accountId ||
      !apiModelVisible.value
    ) {
      return false;
    }
    apiModelAccessStatus.value = allowed ? "matched" : "mismatched";
    return allowed;
  } catch (error) {
    if (
      accessSequence !== apiModelAccessSequence ||
      apiModelAccount.value?.id !== accountId ||
      !apiModelVisible.value
    ) {
      return false;
    }
    apiModelAccessStatus.value = "error";
    apiModelAccessError.value = errorText(error);
    return false;
  }
}

async function handleFetchApiModels(): Promise<void> {
  const account = apiModelAccount.value;
  if (!account) return;
  const accountId = account.id;
  const requestSequence = ++apiModelRequestSequence;
  fetchingApiModels.value = true;
  try {
    const models = await fetchCodexApiKeyModels(accountId);
    if (
      requestSequence !== apiModelRequestSequence ||
      apiModelAccount.value?.id !== accountId ||
      !apiModelVisible.value
    ) {
      return;
    }
    apiModels.value = models;
    if (!models.length) {
      Message.warning(t("API 服务没有返回可用模型"));
    }
  } catch (error) {
    if (requestSequence !== apiModelRequestSequence || apiModelAccount.value?.id !== accountId) return;
    Message.error(`${t("获取模型列表失败")}：${errorText(error)}`);
  } finally {
    if (requestSequence === apiModelRequestSequence) fetchingApiModels.value = false;
  }
}

async function handleSaveApiModel(): Promise<void> {
  const account = apiModelAccount.value;
  const modelId = selectedApiModel.value.trim();
  if (!account || !modelId) {
    Message.warning(t("请选择默认模型"));
    return;
  }
  if (apiModelAccessStatus.value !== "matched") {
    Message.warning(t("当前 Codex 配置不是此 API Key，请先切换到该账号后再设置模型。"));
    return;
  }
  const accountId = account.id;
  const saveSequence = ++apiModelSaveSequence;
  savingApiModel.value = true;
  try {
    await setCodexApiKeyDefaultModel({ accountId, modelId });
    await loadAccounts();
    if (saveSequence !== apiModelSaveSequence || apiModelAccount.value?.id !== accountId) return;
    updateApiModelVisible(false);
    Message.success(t("默认模型已保存并写入 config.toml"));
  } catch (error) {
    if (saveSequence !== apiModelSaveSequence || apiModelAccount.value?.id !== accountId) return;
    Message.error(`${t("保存默认模型失败")}：${errorText(error)}`);
    await checkApiModelAccess();
  } finally {
    if (saveSequence === apiModelSaveSequence) savingApiModel.value = false;
  }
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
  const requestSequence = ++sessionLoadSequence;
  const trashMode = sessionTrashMode.value;
  const titleQuery = sessionSearch.titleQuery;
  const contentQuery = sessionSearch.contentQuery;
  sessionLoading.value = true;
  try {
    if (trashMode) {
      const nextTrashedSessions = await listTrashedSessionsAcrossInstances();
      if (requestSequence !== sessionLoadSequence || trashMode !== sessionTrashMode.value) return;
      trashedSessions.value = nextTrashedSessions;
      sessions.value = [];
      sessionStats.value = [];
    } else {
      const nextSessions = await listSessionsAcrossInstances({
        titleQuery,
        contentQuery,
      });
      if (requestSequence !== sessionLoadSequence || trashMode !== sessionTrashMode.value) return;
      sessions.value = nextSessions;
      trashedSessions.value = [];
      sessionStats.value = nextSessions.map((session) => ({
        sessionId: session.id,
        approximateTokens: Math.ceil((session.charCount || 0) / 4),
        charCount: session.charCount || 0,
      }));
      const firstGroup = nextSessions[0] ? sessionGroupKey(nextSessions[0]) : "";
      expandedSessionGroups.value = firstGroup ? new Set([firstGroup]) : new Set();
    }
    selectedSessionIds.value = new Set();
  } catch (error) {
    if (requestSequence === sessionLoadSequence && !options.silent) {
      Message.error(`加载会话失败：${errorText(error)}`);
    }
  } finally {
    if (requestSequence === sessionLoadSequence) {
      sessionLoading.value = false;
    }
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

async function openSessionFolder(path: string): Promise<void> {
  try {
    await openPathInFileManager(path);
  } catch (error) {
    Message.error(`打开文件夹失败：${errorText(error)}`);
  }
}

function openSessionCopy(session: CodexSessionRecord): void {
  if (sessions.value.length < 2) {
    Message.warning(t("请先新建一个空会话并刷新列表"));
    return;
  }
  sessionCopySource.value = session;
  sessionCopyVisible.value = true;
}

function openSessionContent(session: CodexSessionRecord): void {
  sessionContentTarget.value = session;
  sessionContentVisible.value = true;
}

function handleSessionContentUpdated(): void {
  void loadSessions({ silent: true });
}

async function handleCopySession(targetSessionId: string): Promise<void> {
  const source = sessionCopySource.value;
  if (!source || sessionCopySaving.value) return;
  sessionCopySaving.value = true;
  try {
    const result = await copySessionHistoryAcrossInstances(source.id, targetSessionId);
    sessionCopyVisible.value = false;
    sessionCopySource.value = null;
    Message.success(t("会话数据已复制，目标会话可以继续使用"));
    if (result.warnings.length) {
      Message.warning(`${t("会话已复制，但部分索引同步失败")}：${result.warnings.join("；")}`);
    }
    await loadSessions();
  } catch (error) {
    Message.error(`${t("复制会话失败")}：${errorText(error)}`);
  } finally {
    sessionCopySaving.value = false;
  }
}

function openSessionRename(session: CodexSessionRecord): void {
  sessionRenameTarget.value = session;
  sessionRenameVisible.value = true;
}

async function handleRenameSession(title: string): Promise<void> {
  const session = sessionRenameTarget.value;
  if (!session || sessionRenameSaving.value) return;
  sessionRenameSaving.value = true;
  try {
    const result = await renameSessionAcrossInstances(session.id, title);
    sessionRenameVisible.value = false;
    sessionRenameTarget.value = null;
    Message.success(t("会话名称已修改"));
    if (result.warnings.length) {
      Message.warning(`${t("名称已保存，但部分索引同步失败")}：${result.warnings.join("；")}`);
    }
    await loadSessions();
  } catch (error) {
    Message.error(`${t("修改会话名称失败")}：${errorText(error)}`);
  } finally {
    sessionRenameSaving.value = false;
  }
}

function openSessionDirectory(session: CodexSessionRecord): void {
  sessionDirectoryTarget.value = session;
  sessionDirectoryVisible.value = true;
}

async function handleSessionDirectorySave(projectPath: string): Promise<void> {
  const session = sessionDirectoryTarget.value;
  if (!session || sessionDirectorySaving.value) return;
  sessionDirectorySaving.value = true;
  try {
    const result = await updateSessionWorkingDirectoryAcrossInstances(session.id, projectPath);
    sessionDirectoryVisible.value = false;
    sessionDirectoryTarget.value = null;
    Message.success(t("工作目录已修改"));
    if (result.warnings.length) {
      Message.warning(`${t("工作目录已保存，但部分索引同步失败")}：${result.warnings.join("；")}`);
    }
    await loadSessions();
  } catch (error) {
    Message.error(`${t("修改工作目录失败")}：${errorText(error)}`);
  } finally {
    sessionDirectorySaving.value = false;
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

async function loadConfigEditorFile(): Promise<void> {
  const requestedKind = configEditorKind.value;
  configEditorLoading.value = true;
  try {
    const file = await readCodexConfigFile(requestedKind);
    if (configEditorKind.value !== requestedKind) return;
    configEditorFile.value = file;
    configEditorContent.value = file.content;
  } catch (error) {
    Message.error(`读取配置文件失败：${errorText(error)}`);
  } finally {
    if (configEditorKind.value === requestedKind) configEditorLoading.value = false;
  }
}

async function openConfigEditor(fileKind: CodexConfigFileKind): Promise<void> {
  configEditorKind.value = fileKind;
  configEditorFile.value = null;
  configEditorContent.value = "";
  configEditorVisible.value = true;
  await loadConfigEditorFile();
}

async function formatConfigEditorContent(): Promise<void> {
  configEditorFormatting.value = true;
  try {
    configEditorContent.value = await formatCodexConfigFile(
      configEditorKind.value,
      configEditorContent.value,
    );
    Message.success("格式检查通过");
  } catch (error) {
    Message.error(`格式检查失败：${errorText(error)}`);
  } finally {
    configEditorFormatting.value = false;
  }
}

async function saveConfigEditorContent(): Promise<void> {
  const hadExistingFile = configEditorFile.value?.exists === true;
  const savedKind = configEditorKind.value;
  configEditorSaving.value = true;
  try {
    const file = await writeCodexConfigFile(
      savedKind,
      configEditorContent.value,
    );
    configEditorFile.value = file;
    configEditorContent.value = file.content;
    if (savedKind === "config") await loadAccounts();
    Message.success(`已保存 ${file.name}${hadExistingFile ? "，原文件已自动备份" : ""}`);
  } catch (error) {
    Message.error(`保存配置文件失败：${errorText(error)}`);
  } finally {
    configEditorSaving.value = false;
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
        await loadAccounts();
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
  Modal.warning({
    title: "移入回收站",
    content: `确认将 ${ids.length} 个会话移入回收站？移入后可以在回收站中恢复。`,
    okText: "移入回收站",
    cancelText: "取消",
    hideCancel: false,
    async onOk() {
      await trashSelectedSessions(ids);
    },
  });
}

async function trashSelectedSessions(ids: string[]): Promise<void> {
  try {
    const summary = await moveSessionsToTrashAcrossInstances(ids);
    Message.success(`已移动 ${summary.moved} 个会话到回收站`);
    await loadSessions();
  } catch (error) {
    Message.error(`移入回收站失败：${errorText(error)}`);
  }
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
  addModalTitle.value = "接入新账号";
  addTab.value = tab;
  addModalVisible.value = true;
  if (tab === "oauth" && !oauthUrl.value) {
    void prepareOAuthLogin();
  }
}

async function copyAccountEmail(account: CodexAccount): Promise<void> {
  const email = account.email?.trim();
  if (!email) {
    Message.warning("这个账号没有可复制的邮箱");
    return;
  }
  try {
    await navigator.clipboard.writeText(email);
    Message.success(`已复制邮箱：${email}`);
  } catch (error) {
    Message.error(`复制邮箱失败：${errorText(error)}`);
  }
}

async function openReauthorizeModal(account: CodexAccount): Promise<void> {
  addModalTitle.value = "重新授权账号";
  addTab.value = "oauth";
  addModalVisible.value = true;
  oauthError.value = "";
  oauthCallbackInput.value = "";
  oauthCallbackReceived.value = false;
  if (oauthLoginId.value) {
    await cancelCodexOAuthLogin(oauthLoginId.value).catch(() => undefined);
  }
  oauthLoginId.value = "";
  oauthUrl.value = "";
  await prepareOAuthLogin();
  const label = displayNameForUi(account);
  Message.info(`请重新授权：${label}`);
}

function handleAddTabChange(key: string | number): void {
  if (key === "oauth" && !oauthUrl.value) {
    void prepareOAuthLogin();
  }
}

function switchView(view: ActiveView): void {
  window.scrollTo({ top: 0, left: 0, behavior: "auto" });
  activeView.value = view;
  if (view === "usage") {
    usagePanelMounted.value = true;
  }
  if (view === "apiService") {
    apiServicePanelMounted.value = true;
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

function syncExpandedLayout(): void {
  expandedLayout.value = window.innerWidth >= EXPANDED_LAYOUT_MIN_WIDTH;
}

function handleWindowResize(): void {
  if (windowResizeFrame) return;
  windowResizeFrame = window.requestAnimationFrame(() => {
    windowResizeFrame = undefined;
    syncExpandedLayout();
  });
}

watch([sortedAccounts, () => settings.pageSize], () => {
  if (currentPage.value > totalPages.value) currentPage.value = totalPages.value;
  if (currentPage.value < 1) currentPage.value = 1;
  const visible = new Set(sortedAccounts.value.map((account) => account.id));
  selectedAccountIds.value = new Set([...selectedAccountIds.value].filter((id) => visible.has(id)));
});

watch(
  [
    () => activeView.value,
    () => pagedAccounts.value.map((account) => account.id).join("\u0000"),
  ],
  ([view]) => {
    if (view === "accounts") {
      prefetchVisibleApiKeyBalances();
    } else {
      cancelApiKeyBalancePrefetch();
    }
  },
  { immediate: true },
);

watch(
  () => currentAccount.value?.id,
  () => {
    resetCurrentAccountQuotaTimer();
  },
);

onMounted(() => {
  quotaCountdownTimer = window.setInterval(() => {
    nowMs.value = Date.now();
    refreshOverdueQuotaCountdowns();
    void runDueResetTasks();
  }, 1000);
  apiKeyBalanceRefreshTimer = window.setInterval(() => {
    if (
      activeView.value === "accounts" &&
      apiKeyBalancePrefetchWorkers === 0 &&
      apiKeyBalancePrefetchQueue.length === 0
    ) {
      prefetchVisibleApiKeyBalances();
    }
  }, 60_000);
  void listen<AppUpdateDownloadProgress>(APP_UPDATE_DOWNLOAD_PROGRESS_EVENT, (event) => {
    appUpdateProgress.value = event.payload;
    if (event.payload.status === "completed") {
      appUpdateDownloading.value = false;
      appUpdateCancelling.value = false;
    } else if (event.payload.status === "cancelled") {
      appUpdateDownloading.value = false;
      appUpdateCancelling.value = false;
      appUpdateVisible.value = false;
      Message.info(t("下载已取消"));
    } else if (event.payload.status === "failed") {
      appUpdateDownloading.value = false;
      appUpdateCancelling.value = false;
      appUpdateError.value = event.payload.message || t("更新下载失败");
    }
  }).then((unlisten) => {
    appUpdateUnlisten = unlisten;
  });
  void listen<ApiServiceAutoUpdateEvent>(API_SERVICE_AUTO_UPDATE_EVENT, (event) => {
    apiServiceAutoUpdateEvent.value = event.payload;
    if (event.payload.status === "failed") {
      Message.warning(
        `${t("自动更新失败")}：${event.payload.message || t("请稍后手动检测更新")}`,
      );
    }
  }).then((unlisten) => {
    apiServiceAutoUpdateUnlisten = unlisten;
  });
  void getVersion()
    .then((version) => {
      if (version) appVersion.value = version;
      void checkAppUpdate({ silent: true });
    })
    .catch(() => {
      void checkAppUpdate({ silent: true });
    });
  syncExpandedLayout();
  window.addEventListener("resize", handleWindowResize);
  void initializeResetState();
  void loadAccounts();
  void refreshApiServiceAccountIds();
  void loadSettings({ includeStorage: false });
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
  void listen("codex-account-state-updated", () => {
    void loadAccounts();
  }).then((unlisten) => {
    accountStateUnlisten = unlisten;
  });
});

onUnmounted(() => {
  oauthUnlisten?.();
  appUpdateUnlisten?.();
  apiServiceAutoUpdateUnlisten?.();
  accountStateUnlisten?.();
  window.removeEventListener("resize", handleWindowResize);
  if (windowResizeFrame) window.cancelAnimationFrame(windowResizeFrame);
  if (viewLoadTimer) window.clearTimeout(viewLoadTimer);
  window.removeEventListener("pointerup", handleSortDraftPointerEnd);
  window.removeEventListener("pointercancel", handleSortDraftPointerEnd);
  if (quotaTimer) window.clearTimeout(quotaTimer);
  if (currentAccountQuotaTimer) window.clearTimeout(currentAccountQuotaTimer);
  if (quotaCountdownTimer) window.clearInterval(quotaCountdownTimer);
  if (apiKeyBalanceRefreshTimer) window.clearInterval(apiKeyBalanceRefreshTimer);
  if (countdownSettingsPersistTimer) window.clearTimeout(countdownSettingsPersistTimer);
  cancelApiKeyBalancePrefetch();
  apiKeyBalanceInsecureHttpApprovals.clear();
  apiKeyBalanceInsecureHttpConfirmationPending.clear();
});
</script>

<template>
  <main
    class="app-shell"
    :class="{
      'sidebar-disabled': !settings.sidebarEnabled,
      'accounts-view': activeView === 'accounts',
    }"
  >
    <AppHeader
      :active-view="activeView"
      :sidebar-enabled="settings.sidebarEnabled"
      :accounts-count="accounts.length"
      :oauth-count="oauthCount"
      :api-key-count="apiKeyCount"
      :abnormal-count="abnormalAccountCount"
      :current-account-label="currentAccount ? displayNameForUi(currentAccount) : ''"
      :current-account-error="currentAccount ? quotaErrorLabel(currentAccount) : ''"
      :detecting-current-account="detectingCurrentAccount"
      :refreshing-all-quotas="refreshingAllQuotas"
      :monitor-quota="settings.monitorQuota"
      :privacy-masked="privacyMasked"
      @switch-view="switchView"
      @detect-current-account="handleDetectCurrentAccount"
      @refresh-all-quotas="handleRefreshEveryQuota"
      @toggle-privacy="privacyMasked = !privacyMasked"
      @open-badge-style="badgeStyleVisible = true"
      @open-add="openAddModal"
    />

    <AccountToolbar
      v-if="activeView === 'accounts'"
      :settings="settings"
      :is-current-page-selected="isCurrentPageSelected"
      :account-type-options="accountTypeOptions"
      :account-search-keyword="accountSearchKeyword"
      :show-sort-direction="showSortDirection"
      :current-account-refresh-countdown="currentAccountRefreshCountdown"
      :quota-refresh-countdown="quotaRefreshCountdown"
      @toggle-all="toggleAllAccounts"
      @update:account-search-keyword="accountSearchKeyword = $event"
      @reset-page="currentPage = 1"
      @save-settings="saveSettings"
      @open-sort-editor="openSortEditor"
      @bind-selected-to-api-service="confirmBindSelectedToApiService"
      @batch-export="openBatchExport"
      @open-add="openAddModal"
    />

    <section v-if="activeView === 'accounts'" class="accounts-page-content">
      <AccountList
        :accounts="pagedAccounts"
        :has-any-account="accounts.length > 0"
        :current-id="currentId"
        :selected-account-ids="selectedAccountIds"
        :settings="settings"
        :expanded-layout="expandedLayout"
        :loading="loading"
        :switching-id="switchingId"
        :deleting-id="deletingId"
        :exporting-id="exportingId"
        :quota-refreshing-id="quotaRefreshingId"
        :api-key-balance-states="apiKeyBalanceStates"
        :privacy-masked="privacyMasked"
        :status-clock-ms="accountStatusClockMs"
        :api-service-account-ids="apiServiceAccountIds"
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
        @open-models="openApiModels"
        @switch-account="handleSwitch"
        @refresh-quota="handleRefreshQuota"
        @refresh-api-balance="handleRefreshApiKeyBalance"
        @open-export="openExport"
        @confirm-delete="confirmDelete"
        @open-add="openAddModal"
        @copy-email="copyAccountEmail"
        @reauthorize="openReauthorizeModal"
      />

      <div v-if="totalPages > 1" class="pagination-bar">
        <div class="pagination-selection">
          <span>{{ t("已选择") }} {{ selectedAccountIdList.length }} {{ t("个账号") }}</span>
          <a-button
            v-if="selectedAccountIdList.length"
            size="mini"
            type="text"
            @click="clearSelectedAccounts"
          >
            {{ t("清空选择") }}
          </a-button>
        </div>
        <a-pagination
          v-model:current="currentPage"
          :total="sortedAccounts.length"
          :page-size="settings.pageSize"
        />
        <div class="pagination-summary">
          <span>{{ t("共") }} {{ sortedAccounts.length }} {{ t("条") }}</span>
          <a-select
            v-model="settings.pageSize"
            size="small"
            class="pagination-page-size"
            @change="() => { currentPage = 1; saveSettings(); }"
          >
            <a-option :value="20">20 {{ t("条/页") }}</a-option>
            <a-option :value="50">50 {{ t("条/页") }}</a-option>
            <a-option :value="100">100 {{ t("条/页") }}</a-option>
            <a-option :value="200">200 {{ t("条/页") }}</a-option>
          </a-select>
        </div>
      </div>
    </section>

    <SortEditorModal
      v-model:visible="sortEditorVisible"
      :accounts="sortDraftAccounts"
      :current-id="currentId"
      :saving="savingSettings"
      :sort-draft-dragging-id="sortDraftDraggingId"
      :sort-draft-over-id="sortDraftOverId"
      :display-name="displayNameForUi"
      :is-api-key-account="isApiKeyAccount"
      :plan-label="planLabel"
      :plan-class="planClass"
      @close="closeSortEditor"
      @save="saveSortEditor"
      @pointer-start="handleSortDraftPointerStart"
      @pointer-enter="handleSortDraftPointerEnter"
      @move-step="moveSortDraftByStep"
    />

    <BadgeStyleModal
      v-model:visible="badgeStyleVisible"
      :settings="settings"
      :saving="savingSettings"
      @save="saveSettings"
    />

    <ResetPanel
      v-if="activeView === 'resets'"
      :state="resetState"
      :accounts="accounts"
      :now-ms="nowMs"
      :loading="resetStateLoading"
      :saving="resetStateSaving"
      :updating-schedule-ids="updatingResetScheduleIds"
      :cancelling-schedule-ids="cancellingResetScheduleIds"
      :deleting-log-ids="deletingResetLogIds"
      :clearing-logs="clearingResetLogs"
      @refresh="loadResetState"
      @edit-schedule="handleEditScheduledReset"
      @cancel-schedule="handleCancelScheduledReset"
      @delete-log="handleDeleteResetLog"
      @clear-logs="handleClearResetLogs"
    />

    <SessionPanel
      v-if="activeView === 'sessions'"
      v-model:session-trash-mode="sessionTrashMode"
      :session-search="sessionSearch"
      :session-loading="sessionLoading"
      :backup-working="backupWorking"
      :backup-button-text="backupButtonText"
      :session-backup-loading="sessionBackupLoading"
      :session-repairing="sessionRepairing"
      :active-session-ids="activeSessionIds"
      :all-sessions-selected="allSessionsSelected"
      :selected-session-ids="selectedSessionIds"
      :selected-session-id-list="selectedSessionIdList"
      :expanded-session-groups="expandedSessionGroups"
      :session-groups="sessionGroups"
      :sessions="sessions"
      :trashed-sessions="trashedSessions"
      :format-time="formatTime"
      :session-approx-tokens="sessionApproxTokens"
      :is-session-group-selected="isSessionGroupSelected"
      @load-sessions="loadSessions"
      @toggle-all-sessions="toggleAllSessions"
      @export-session-backup="handleExportSessionBackup"
      @open-session-restore-modal="openSessionRestoreModal"
      @repair-sessions="handleRepairSessions"
      @trash-sessions="handleTrashSessions"
      @restore-sessions="handleRestoreSessions"
      @toggle-session-group-expanded="toggleSessionGroupExpanded"
      @toggle-session-group-selection="toggleSessionGroupSelection"
      @toggle-session="toggleSession"
      @open-session-folder="openSessionFolder"
      @view-session-content="openSessionContent"
      @copy-session="openSessionCopy"
      @rename-session="openSessionRename"
      @edit-session-directory="openSessionDirectory"
    />

    <SessionContentModal
      v-model:visible="sessionContentVisible"
      :session="sessionContentTarget"
      @session-updated="handleSessionContentUpdated"
    />

    <SessionCopyModal
      v-model:visible="sessionCopyVisible"
      :source="sessionCopySource"
      :sessions="sessions"
      :saving="sessionCopySaving"
      @save="handleCopySession"
    />

    <SessionRenameModal
      v-model:visible="sessionRenameVisible"
      :session="sessionRenameTarget"
      :saving="sessionRenameSaving"
      @save="handleRenameSession"
    />

    <SessionDirectoryModal
      v-model:visible="sessionDirectoryVisible"
      :session="sessionDirectoryTarget"
      :saving="sessionDirectorySaving"
      @save="handleSessionDirectorySave"
    />

    <UsagePanel
      v-if="usagePanelMounted"
      v-show="activeView === 'usage'"
      :active="activeView === 'usage'"
    />

    <ApiServicePanel
      v-if="apiServicePanelMounted"
      v-show="activeView === 'apiService'"
      :active="activeView === 'apiService'"
      :accounts="apiServiceAccounts"
      :settings="settings"
      :auto-update-event="apiServiceAutoUpdateEvent"
      @account-added="handleApiServiceAccountAdded"
      @bound-accounts-changed="refreshApiServiceAccountIds"
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
      @edit-codex-file="openConfigEditor"
      @reset-config="confirmResetConfig"
      @export-backup="handleExportBackup"
      @refresh-backups="loadBackups"
      @restore-backup="handleRestoreBackup"
      @delete-backup="handleDeleteBackup"
      @open-push-settings="switchView('pushSettings')"
    />

    <PushSettingsPanel
      v-if="activeView === 'pushSettings'"
      :accounts="apiServiceAccounts"
      :display-name="displayNameForUi"
      :plan-label="planLabel"
      :plan-class="planClass"
      :privacy-masked="privacyMasked"
      @back="switchView('settings')"
      @accounts-refreshed="loadAccounts"
    />

    <AboutPanel
      v-if="activeView === 'about'"
      :app-version="appVersion"
      :checking-update="checkingAppUpdate"
      @open-github-profile="openGithubProfile"
      @open-repository="openRepository"
      @open-sponsor-page="openSponsorPage"
      @open-feedback-page="openFeedbackPage"
      @check-update="checkAppUpdate"
    />

    <AppUpdateModal
      :visible="appUpdateVisible"
      :info="appUpdateInfo"
      :progress="appUpdateProgress"
      :result="appUpdateResult"
      :error="appUpdateError"
      :downloading="appUpdateDownloading"
      :cancelling="appUpdateCancelling"
      :opening="appUpdateOpening"
      @close="closeAppUpdateModal"
      @cancel="cancelCurrentAppUpdate"
      @retry="startAppUpdateDownload()"
      @open-installer="openDownloadedAppUpdate"
      @open-releases="openAppUpdateReleases"
    />

    <CodexConfigEditorModal
      v-model:visible="configEditorVisible"
      v-model:content="configEditorContent"
      :file-kind="configEditorKind"
      :file="configEditorFile"
      :loading="configEditorLoading"
      :saving="configEditorSaving"
      :formatting="configEditorFormatting"
      @reload="loadConfigEditorFile"
      @format="formatConfigEditorContent"
      @save="saveConfigEditorContent"
    />

    <SessionRestoreModal
      v-model:visible="sessionRestoreVisible"
      :backups="sessionBackupFiles"
      :loading="sessionBackupLoading"
      :backup-working="backupWorking"
      @restore="handleRestoreSessionBackup"
      @backup-now="handleExportSessionBackup"
    />

    <BackupProgressModal
      v-model:visible="backupProgressVisible"
      :title="backupProgressTitle"
      :progress="backupProgress"
      :status="backupProgressStatus"
      :message="backupProgressMessage"
    />

    <AddAccountModal
      v-model:visible="addModalVisible"
      :title="addModalTitle"
      :active-tab="addTab"
      :oauth-url="oauthUrl"
      :oauth-callback-input="oauthCallbackInput"
      :oauth-login-id="oauthLoginId"
      :oauth-preparing="oauthPreparing"
      :oauth-completing="oauthCompleting"
      :oauth-error="oauthError"
      :oauth-callback-received="oauthCallbackReceived"
      :token-input="tokenInput"
      :importing="importing"
      :saving-api-key="savingApiKey"
      :api-key-form="apiKeyForm"
      :oauth-accounts="oauthAccounts"
      :display-name="displayNameForUi"
      @update:active-tab="addTab = $event"
      @update:oauth-callback-input="oauthCallbackInput = $event"
      @update:token-input="tokenInput = $event"
      @tab-change="handleAddTabChange"
      @start-or-open-oauth="startOrOpenOAuthUrl"
      @copy-oauth-url="copyOAuthUrl"
      @submit-oauth-callback="handleOAuthCallbackSubmit"
      @local-import="handleLocalImport"
      @files-import="handleFileImport"
      @token-import="handleTokenImport"
      @api-key-add="handleApiKeyAdd"
    />

    <EditAccountModal
      v-model:visible="editVisible"
      :title="editTitle"
      :active-tab="editTab"
      :editing-account="editingAccount"
      :edit-form="editForm"
      :edit-json-text="editJsonText"
      :editing="editing"
      :tag-options="allAccountTags"
      :is-api-key-account="isApiKeyAccount"
      @update:active-tab="editTab = $event"
      @update:edit-json-text="editJsonText = $event"
      @save="handleEditSave"
    />

    <ExportJsonModal
      v-model:visible="exportVisible"
      :title="t('导出 JSON')"
      :export-format="exportFormat"
      :export-format-options="exportFormatOptions"
      :preview-visible="exportPreviewVisible"
      :text="exportText"
      :summary="exportJsonSummary(exportText)"
      @update:export-format="exportFormat = $event"
      @update:preview-visible="exportPreviewVisible = $event"
      @format-change="refreshExportText"
      @copy="copyExportText"
      @download="downloadExportText"
    />

    <ExportJsonModal
      v-model:visible="batchExportVisible"
      :title="t('批量导出 JSON')"
      :export-format="exportFormat"
      :export-format-options="exportFormatOptions"
      :preview-visible="batchExportPreviewVisible"
      :text="batchExportText"
      :summary="exportJsonSummary(batchExportText)"
      width="820px"
      @update:export-format="exportFormat = $event"
      @update:preview-visible="batchExportPreviewVisible = $event"
      @format-change="refreshBatchExportText"
      @copy="copyBatchExportText"
      @download="downloadBatchExportText"
    />

    <PhoneModal
      v-model:visible="phoneVisible"
      :account="phoneAccount"
      :phone-form="phoneForm"
      :saving="savingPhone"
      :display-name="displayNameForUi"
      @save="handlePhoneSave"
    />

    <OAuthBindingModal
      v-model:visible="bindingVisible"
      :binding-form="bindingForm"
      :saving="savingBinding"
      :oauth-accounts="oauthAccounts"
      :display-name="displayNameForUi"
      :is-free-plan-account="isFreePlanAccount"
      :quota-color="quotaColor"
      :quota-window-label="quotaWindowLabel"
      :quota-reset-label="quotaResetLabel"
      :plan-label="planLabel"
      :plan-class="planClass"
      @save="handleBindingSave"
    />

    <ApiKeyModelModal
      :visible="apiModelVisible"
      v-model:selected-model="selectedApiModel"
      :account="apiModelAccount"
      :models="apiModels"
      :loading="fetchingApiModels"
      :saving="savingApiModel"
      :access-status="apiModelAccessStatus"
      :access-error="apiModelAccessError"
      @update:visible="updateApiModelVisible"
      @check-access="checkApiModelAccess"
      @fetch="handleFetchApiModels"
      @save="handleSaveApiModel"
    />

    <ResetCreditModal
      v-model:visible="resetCreditVisible"
      :account="resetCreditAccount"
      :records="resetCreditRecordsForModal"
      :quota-refreshing-id="quotaRefreshingId"
      :display-name="displayNameForUi"
      :reset-credit-count="resetCreditCount"
      :is-available-reset-credit="isAvailableResetCredit"
      :reset-credit-status-key="resetCreditStatusKey"
      :reset-credit-status-label="resetCreditStatusLabel"
      :format-reset-credit-date="formatResetCreditDate"
      :scheduled-reset="scheduledResetForModal"
      :reset-state-busy="resetStateLoading || resetStateSaving"
      @consume="handleConsumeSelectedResetCredit"
      @open-schedule="handleOpenResetSchedule"
      @view-schedules="handleViewResetSchedules"
    />

    <ResetScheduleModal
      :visible="resetScheduleVisible"
      :account-label="resetScheduleAccountLabel"
      :saving="resetStateSaving"
      :mode="resetScheduleMode"
      :initial-scheduled-at="resetScheduleInitialAt"
      @update:visible="updateResetScheduleVisible"
      @save="handleSaveResetSchedule"
    />

    <SessionRepairModal
      v-model:visible="repairVisible"
      v-model:repair-mode="repairMode"
      v-model:repair-target-instance-id="repairTargetInstanceId"
      v-model:repair-instance-scope="repairInstanceScope"
      v-model:repair-session-scope="repairSessionScope"
      :selected-count="selectedSessionIdList.length"
      :total-count="sessions.length"
      :repair-instances="repairInstances"
      :effective-repair-session-scope="effectiveRepairSessionScope"
      :repair-result="repairResult"
      :session-repairing="sessionRepairing"
      @run="runRepairSessions"
    />
  </main>
</template>
