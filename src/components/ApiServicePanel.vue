<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { isSubscriptionExpired } from "../accountStatus";
import { addCodexAccountWithApiKey, openExternalUrl, type CodexSwitcherSettings } from "../services/codex";
import { t } from "../i18n";
import { hasAnyQuotaWindow, hasQuotaWindow } from "../quota";
import type { CodexAccount } from "../types/codex";
import PlanBadge from "./PlanBadge.vue";
import {
  API_SERVICE_DOWNLOAD_PROGRESS_EVENT,
  activateApiServiceRuntime,
  bindApiServiceAccounts,
  cancelApiServiceDownload,
  checkApiServiceUpdate,
  deleteApiServiceBoundAccounts,
  deleteApiServiceRuntime,
  downloadApiServiceUpdate,
  getApiServiceState,
  importApiServiceRuntime,
  isCurrentApiServiceAccount,
  listApiServiceBoundAccounts,
  resetApiService,
  startApiService,
  stopApiService,
  updateApiServiceSettings,
  type ApiServiceAutoUpdateEvent,
  type ApiServiceBoundAccount,
  type ApiServiceDownloadProgress,
  type ApiServiceState,
  type ApiServiceUpdateInfo,
} from "../services/apiService";

const props = defineProps<{
  accounts: CodexAccount[];
  settings: CodexSwitcherSettings;
  active: boolean;
  autoUpdateEvent?: ApiServiceAutoUpdateEvent | null;
}>();

const emit = defineEmits<{
  (event: "account-added", account: CodexAccount): void;
}>();

const state = ref<ApiServiceState | null>(null);
const updateInfo = ref<ApiServiceUpdateInfo | null>(null);
const loading = ref(false);
const saving = ref(false);
const addingAccount = ref(false);
const starting = ref(false);
const stopping = ref(false);
const resetting = ref(false);
const bindingAccounts = ref(false);
const deletingBoundAccounts = ref(false);
const checking = ref(false);
const downloading = ref(false);
const bindVisible = ref(false);
const deleteVisible = ref(false);
const versionVisible = ref(false);
const importingRuntime = ref(false);
const activatingRuntimeId = ref("");
const deletingRuntimeId = ref("");
const selectedBindIds = ref<Set<string>>(new Set());
const selectedDeleteIds = ref<Set<string>>(new Set());
const bindSearchKeyword = ref("");
const boundAccounts = ref<ApiServiceBoundAccount[]>([]);
const progress = ref<ApiServiceDownloadProgress | null>(null);
let unlistenProgress: UnlistenFn | null = null;
let countdownTimer: number | undefined;
let progressClearTimer: number | undefined;
let panelMounted = false;
const countdownNow = ref(Date.now());

const form = reactive({
  port: 17877,
  managementKey: "",
  apiKeys: [] as string[],
  autoUpdate: false,
  autoUpdateIntervalHours: 24,
});
const visibleApiKeyIndexes = ref<Set<number>>(new Set());

const serviceReady = computed(() => Boolean(state.value?.installed));
const running = computed(() => Boolean(state.value?.service.running));
const currentRuntime = computed(() =>
  state.value?.runtimes.find((runtime) => runtime.id === state.value?.activeVersion) || null,
);
const runtimeOperationBusy = computed(() =>
  Boolean(importingRuntime.value || activatingRuntimeId.value || deletingRuntimeId.value),
);
const canDownloadUpdate = computed(() =>
  Boolean(updateInfo.value?.downloadUrl && updateInfo.value.canApply),
);
const progressPercent = computed(() => {
  const status = progress.value?.status;
  if (status === "done") return 1;
  if (status === "installing") return 0.96;
  if (status === "starting") return 0.06;
  const total = progress.value?.totalBytes || 0;
  const downloaded = progress.value?.downloadedBytes || 0;
  if (!total) {
    if (!downloading.value) return 0;
    if (downloaded <= 0) return 0.12;
    const downloadedMb = downloaded / 1024 / 1024;
    return Math.min(0.88, 0.12 + Math.log2(downloadedMb + 1) * 0.12);
  }
  return Math.max(0.08, Math.min(0.94, downloaded / total));
});
const progressStatus = computed(() => {
  if (progress.value?.status === "failed") return "danger";
  if (progress.value?.status === "done") return "success";
  return "normal";
});
const savedAutoUpdateEnabled = computed(() => Boolean(state.value?.installed && state.value.settings.autoUpdate));
const autoUpdateFormChanged = computed(() =>
  Boolean(state.value) &&
  (form.autoUpdate !== Boolean(state.value?.settings.autoUpdate) ||
    Math.max(1, Math.trunc(Number(form.autoUpdateIntervalHours) || 24)) !==
      Math.max(1, Number(state.value?.settings.autoUpdateIntervalHours || 24))),
);
const autoUpdateToggleChanged = computed(() =>
  Boolean(state.value) && form.autoUpdate !== Boolean(state.value?.settings.autoUpdate),
);
const autoUpdateCountdown = computed(() => {
  if (autoUpdateToggleChanged.value) return form.autoUpdate ? t("保存后生效") : t("保存后关闭");
  if (autoUpdateFormChanged.value) return t("保存后生效");
  if (!savedAutoUpdateEnabled.value) return t("已关闭");
  const lastCheck = Number(state.value?.settings.lastUpdateCheckAt || 0);
  if (!lastCheck) return t("即将检测");
  const intervalSeconds = Math.max(1, Number(state.value?.settings.autoUpdateIntervalHours || 24)) * 3600;
  const remainingSeconds = Math.max(0, Math.ceil(lastCheck + intervalSeconds - countdownNow.value / 1000));
  if (remainingSeconds <= 0) return t("等待后台检测");
  return `${t("距下次检测")} ${formatCountdown(remainingSeconds)}`;
});
const progressDetail = computed(() => {
  if (!progress.value) return "";
  const total = progress.value.totalBytes || 0;
  const downloaded = progress.value.downloadedBytes || 0;
  if (progress.value.status === "installing") return t("安装中");
  if (progress.value.status === "done") return "100%";
  if (progress.value.status === "failed") return t("失败");
  if (progress.value.status === "cancelled") return t("已取消");
  if (!total) return downloaded > 0 ? `${formatBytes(downloaded)} · ${t("正在下载")}` : t("准备中");
  return `${formatBytes(downloaded)} / ${formatBytes(total)}`;
});
const serviceStatusText = computed(() => {
  if (running.value) return `${t("运行中")} · PID ${state.value?.service.pid || "--"}`;
  if (serviceReady.value) return t("已安装，当前未启动");
  return t("未安装，首次开启时会下载服务");
});
const apiBaseUrl = computed(() => `http://127.0.0.1:${form.port || 17877}/v1`);
const firstApiKey = computed(() => form.apiKeys.find((key) => key.trim())?.trim() || "");
const bindableAccounts = computed(() =>
  props.accounts.filter((account) => !isCurrentApiServiceAccount(account, state.value)),
);
const filteredBindableAccounts = computed(() => {
  const keyword = bindSearchKeyword.value.trim().toLocaleLowerCase();
  if (!keyword) return bindableAccounts.value;
  return bindableAccounts.value.filter((account) =>
    [account.account_name, account.email]
      .some((value) => (value || "").toLocaleLowerCase().includes(keyword)),
  );
});
const selectedBindCount = computed(
  () => bindableAccounts.value.filter((account) => selectedBindIds.value.has(account.id)).length,
);
const selectedVisibleBindCount = computed(
  () => filteredBindableAccounts.value.filter((account) => selectedBindIds.value.has(account.id)).length,
);
const allBindSelected = computed(
  () =>
    filteredBindableAccounts.value.length > 0 &&
    selectedVisibleBindCount.value === filteredBindableAccounts.value.length,
);
const bindSelectionIndeterminate = computed(
  () =>
    selectedVisibleBindCount.value > 0 &&
    selectedVisibleBindCount.value < filteredBindableAccounts.value.length,
);
const selectedDeleteCount = computed(
  () => boundAccounts.value.filter((account) => selectedDeleteIds.value.has(account.id)).length,
);
const allDeleteSelected = computed(
  () => boundAccounts.value.length > 0 && selectedDeleteCount.value === boundAccounts.value.length,
);
const deleteSelectionIndeterminate = computed(
  () => selectedDeleteCount.value > 0 && selectedDeleteCount.value < boundAccounts.value.length,
);

function syncForm(next: ApiServiceState): void {
  form.port = next.settings.port || 17877;
  form.managementKey = next.settings.managementKey || "";
  form.apiKeys = next.settings.apiKeys?.length ? [...next.settings.apiKeys] : generateApiKeys();
  visibleApiKeyIndexes.value = new Set();
  form.autoUpdate = Boolean(next.settings.autoUpdate);
  form.autoUpdateIntervalHours = Math.max(1, Number(next.settings.autoUpdateIntervalHours || 24));
}

function translatedTemplate(template: string, values: Record<string, string | number>): string {
  return Object.entries(values).reduce(
    (message, [key, value]) => message.replaceAll(`{${key}}`, String(value)),
    t(template),
  );
}

function runtimeMaintenanceMessage(next: ApiServiceState | null): string {
  const count = Number(next?.maintenanceOldRuntimeCount || 0);
  if (count <= 2) return "";
  return translatedTemplate(
    "旧版本清理尚未完成：当前有 {count} 个旧版本，最多应保留 {limit} 个。请稍后重试删除。",
    { count, limit: 2 },
  );
}

function warnRuntimeMaintenance(next: ApiServiceState): void {
  const warning = runtimeMaintenanceMessage(next);
  if (warning) Message.warning(warning);
}

async function refreshState(silent = false, syncSettings = true): Promise<boolean> {
  loading.value = !silent;
  try {
    const next = await getApiServiceState();
    state.value = next;
    if (syncSettings) syncForm(next);
    return true;
  } catch (error) {
    if (!silent) Message.error(`加载 API 服务失败：${errorText(error)}`);
    return false;
  } finally {
    loading.value = false;
  }
}

async function saveSettings(): Promise<void> {
  saving.value = true;
  try {
    const next = await updateApiServiceSettings({
      port: Math.max(1, Math.min(65535, Math.trunc(Number(form.port) || 17877))),
      managementKey: form.managementKey.trim(),
      apiKeys: form.apiKeys.map((key) => key.trim()).filter(Boolean),
      autoUpdate: form.autoUpdate,
      autoUpdateIntervalHours: Math.max(1, Math.trunc(Number(form.autoUpdateIntervalHours) || 24)),
    });
    state.value = next;
    syncForm(next);
    Message.success(t("API 服务配置已保存"));
  } catch (error) {
    Message.error(`保存失败：${errorText(error)}`);
  } finally {
    saving.value = false;
  }
}

async function addToAccountOverview(): Promise<void> {
  if (!serviceReady.value) {
    Message.warning(t("请先下载并开启 API 服务"));
    return;
  }
  const apiKey = firstApiKey.value;
  if (!apiKey) {
    Message.error(t("请先添加一个 API 密钥"));
    return;
  }
  addingAccount.value = true;
  try {
    const account = await addCodexAccountWithApiKey({
      apiKey,
      apiBaseUrl: apiBaseUrl.value,
      apiProviderName: "CLIProxyAPI",
      apiOfficialUrl: state.value?.service.managementUrl || "",
      accountName: "本地 API 服务",
    });
    emit("account-added", account);
    Message.success(`已添加 ${account.account_name || account.email}`);
  } catch (error) {
    Message.error(`添加到账号总览失败：${errorText(error)}`);
  } finally {
    addingAccount.value = false;
  }
}

async function start(): Promise<void> {
  starting.value = true;
  progress.value = null;
  try {
    const next = await startApiService();
    state.value = next;
    syncForm(next);
    progress.value = null;
    Message.success(t("API 服务已开启"));
  } catch (error) {
    Message.error(`开启失败：${errorText(error)}`);
  } finally {
    starting.value = false;
    downloading.value = false;
  }
}

async function stop(): Promise<void> {
  stopping.value = true;
  try {
    const next = await stopApiService();
    state.value = next;
    syncForm(next);
    Message.success(t("API 服务已停止"));
  } catch (error) {
    Message.error(`停止失败：${errorText(error)}`);
  } finally {
    stopping.value = false;
  }
}

function accountDisplayName(account: CodexAccount): string {
  return account.account_name || account.email || account.id;
}

function isApiKeyAccount(account: CodexAccount): boolean {
  return account.auth_mode === "apikey" || Boolean(account.openai_api_key || account.openaiApiKey);
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

function planLabel(account: CodexAccount): string {
  if (isApiKeyAccount(account)) return "API_KEY";
  const base = planDisplayName(effectivePlanKey(account));
  if (base !== "PRO") return base;
  const authPlan = normalizeAuthFilePlan(account.auth_file_plan_type || account.plan_type);
  return authPlan === "prolite" ? "PRO 5X" : "PRO 20X";
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

function planClass(account: CodexAccount): string {
  const badgeKey = badgeTypeKey(account);
  const styleName =
    props.settings.badgeStyles?.[badgeKey] ||
    props.settings.badgeStyle ||
    "classic";
  const style = `badge-${styleName}`;
  if (isApiKeyAccount(account)) return `api ${style}`;
  const key = effectivePlanKey(account);
  if (key === "pro") {
    const proClass =
      normalizeAuthFilePlan(account.auth_file_plan_type || account.plan_type) === "prolite"
        ? "pro-lite"
        : "pro-max";
    return `${proClass} ${style}`;
  }
  return `${key} ${style}`;
}

function normalizedEmail(value?: string | null): string {
  return (value || "").trim().toLowerCase();
}

function effectivePlanKey(account: CodexAccount): string {
  if (
    !isApiKeyAccount(account) &&
    isSubscriptionExpired(account.subscription_active_until)
  ) {
    return "free";
  }
  return normalizePlanKey(account.plan_type);
}

function boundSourceAccount(account: ApiServiceBoundAccount): CodexAccount | undefined {
  if (account.accountId) {
    const matched = props.accounts.find((item) => item.id === account.accountId);
    if (matched) return matched;
  }
  const email = normalizedEmail(account.email);
  if (!email) return undefined;
  return props.accounts.find((item) => normalizedEmail(item.email) === email);
}

function boundPlanLabel(account: ApiServiceBoundAccount): string {
  const source = boundSourceAccount(account);
  return source ? planLabel(source) : "FREE";
}

function boundPlanClass(account: ApiServiceBoundAccount): string {
  const source = boundSourceAccount(account);
  if (source) return planClass(source);
  const styleName =
    props.settings.badgeStyles?.free ||
    props.settings.badgeStyle ||
    "classic";
  return `free badge-${styleName}`;
}

function isFreePlanAccount(account: CodexAccount): boolean {
  return !isApiKeyAccount(account) && effectivePlanKey(account) === "free";
}

function quotaWindowLabel(minutes?: number, fallback = "5 小时窗口"): string {
  if (!minutes || !Number.isFinite(minutes)) return fallback;
  if (minutes % (60 * 24) === 0) return t(`${minutes / 60 / 24} 天窗口`);
  if (minutes % 60 === 0) return t(`${minutes / 60} 小时窗口`);
  return t(`${minutes} 分钟窗口`);
}

function quotaColor(value?: number): string {
  const percentage = value ?? 0;
  if (percentage >= 70) return "#22c55e";
  if (percentage >= 40) return "#f59e0b";
  return "#ef4444";
}

function normalizeDate(value?: string | number | null): Date | undefined {
  if (value === undefined || value === null || value === "") return undefined;
  const numeric = typeof value === "number" ? value : Number(value);
  const date = Number.isFinite(numeric)
    ? new Date(numeric > 10_000_000_000 ? numeric : numeric * 1000)
    : new Date(String(value));
  return Number.isNaN(date.getTime()) ? undefined : date;
}

function quotaResetLeftLabel(value?: string | number): string {
  const date = normalizeDate(value);
  if (!date) return "--";
  const diff = date.getTime() - Date.now();
  const abs = Math.max(0, diff);
  const day = Math.floor(abs / 86_400_000);
  const hour = Math.floor((abs % 86_400_000) / 3_600_000);
  const minute = Math.floor((abs % 3_600_000) / 60_000);
  return day > 0 ? `${day}d ${hour}h ${minute}m` : `${hour}h ${minute}m`;
}

function quotaResetDateLabel(value?: string | number): string {
  const date = normalizeDate(value);
  if (!date) return t("等待刷新");
  const pad = (input: number) => String(input).padStart(2, "0");
  return t(`更新 ${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`);
}

async function openBindAccounts(): Promise<void> {
  if (!(await loadBoundAccounts())) return;
  const boundAccountIds = new Set(
    boundAccounts.value
      .map((account) => account.accountId || boundSourceAccount(account)?.id)
      .filter((accountId): accountId is string => Boolean(accountId)),
  );
  selectedBindIds.value = new Set(
    bindableAccounts.value
      .filter((account) => boundAccountIds.has(account.id))
      .map((account) => account.id),
  );
  bindSearchKeyword.value = "";
  bindVisible.value = true;
}

async function bindSelectedAccounts(): Promise<void> {
  const ids = [...selectedBindIds.value];
  if (!ids.length) {
    Message.warning(t("请选择要绑定的账号"));
    return;
  }
  bindingAccounts.value = true;
  try {
    const summary = await bindApiServiceAccounts(ids);
    bindVisible.value = false;
    Message.success(
      `已绑定 ${summary.count} 个账号到 API 服务（OAuth ${summary.oauthCount}，API Key ${summary.apiKeyCount}）`,
    );
    await loadBoundAccounts();
  } catch (error) {
    Message.error(`绑定失败：${errorText(error)}`);
  } finally {
    bindingAccounts.value = false;
  }
}

async function openDeleteAccounts(): Promise<void> {
  if (!(await loadBoundAccounts())) return;
  selectedDeleteIds.value = new Set(boundAccounts.value.map((account) => account.id));
  deleteVisible.value = true;
}

async function loadBoundAccounts(): Promise<boolean> {
  try {
    boundAccounts.value = await listApiServiceBoundAccounts();
    return true;
  } catch (error) {
    boundAccounts.value = [];
    Message.error(`读取 API 服务账号失败：${errorText(error)}`);
    return false;
  }
}

async function deleteSelectedBoundAccounts(): Promise<void> {
  const ids = [...selectedDeleteIds.value];
  if (!ids.length) {
    Message.warning(t("请选择要删除的 API 服务账号"));
    return;
  }
  deletingBoundAccounts.value = true;
  try {
    const summary = await deleteApiServiceBoundAccounts(ids);
    await loadBoundAccounts();
    selectedDeleteIds.value = new Set();
    deleteVisible.value = false;
    Message.success(`已删除 ${summary.count} 个 API 服务账号`);
  } catch (error) {
    Message.error(`删除失败：${errorText(error)}`);
  } finally {
    deletingBoundAccounts.value = false;
  }
}

function toggleBindAccount(accountId: string, checked: boolean): void {
  const next = new Set(selectedBindIds.value);
  if (checked) next.add(accountId);
  else next.delete(accountId);
  selectedBindIds.value = next;
}

function toggleAllBindAccounts(checked: boolean): void {
  const next = new Set(selectedBindIds.value);
  for (const account of filteredBindableAccounts.value) {
    if (checked) next.add(account.id);
    else next.delete(account.id);
  }
  selectedBindIds.value = next;
}

function toggleDeleteAccount(id: string, checked: boolean): void {
  const next = new Set(selectedDeleteIds.value);
  if (checked) next.add(id);
  else next.delete(id);
  selectedDeleteIds.value = next;
}

function toggleAllDeleteAccounts(checked: boolean): void {
  selectedDeleteIds.value = checked
    ? new Set(boundAccounts.value.map((account) => account.id))
    : new Set();
}

function resetService(): void {
  Modal.warning({
    title: "重置 API 服务",
    content: "将先停止 API 服务，然后删除 ~/.codex_switcher/api-service 下的运行时、配置、工作区和下载缓存。此操作不会删除账号总览里的账号，是否继续？",
    okText: "确认重置",
    cancelText: "取消",
    hideCancel: false,
    async onOk() {
      resetting.value = true;
      try {
        const next = await resetApiService();
        state.value = next;
        updateInfo.value = null;
        progress.value = null;
        downloading.value = false;
        syncForm(next);
        Message.success("API 服务已重置");
      } catch (error) {
        Message.error(`重置失败：${errorText(error)}`);
      } finally {
        resetting.value = false;
      }
    },
  });
}

async function checkUpdate(): Promise<void> {
  checking.value = true;
  try {
    updateInfo.value = await checkApiServiceUpdate();
    if (progress.value?.status === "failed") progress.value = null;
    if (updateInfo.value.hasUpdate) {
      Message.info(`发现新版本 ${updateInfo.value.latestVersion}`);
    } else {
      Message.success(`当前已是最新版本 ${updateInfo.value.latestVersion}`);
    }
    await refreshState(true, false);
  } catch (error) {
    const detail = errorText(error);
    if (detail.includes("403") || detail.toLowerCase().includes("rate limit")) {
      Message.warning(t("GitHub 更新接口当前限流，可打开“版本管理”导入本地安装包"));
    } else {
      Message.error(`检测更新失败：${detail}`);
    }
  } finally {
    checking.value = false;
  }
}

async function openVersionManager(): Promise<void> {
  if (!(await refreshState(false, false))) return;
  versionVisible.value = true;
}

async function chooseRuntimePackage(): Promise<void> {
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [
        {
          name: t("CLIProxyAPI 安装包"),
          extensions: ["gz", "tgz", "zip"],
        },
      ],
    });
    if (typeof selected !== "string" || !selected) return;
    Modal.warning({
      title: t("导入本地版本包"),
      content: t(
        "手动导入用于 GitHub 限流或网络不可用的情况，不会通过 GitHub checksums 在线校验。请只导入可信的 CLIProxyAPI 官方安装包。导入成功后会自动使用本机最高版本，并仅保留当前版本和 2 个旧版本。",
      ),
      okText: t("确认导入"),
      cancelText: t("取消"),
      hideCancel: false,
      async onOk() {
        importingRuntime.value = true;
        try {
          const next = await importApiServiceRuntime(selected);
          state.value = next;
          updateInfo.value = null;
          Message.success(t("版本包已导入，已切换到本机最新版本"));
          warnRuntimeMaintenance(next);
        } catch (error) {
          Message.error(
            translatedTemplate("导入失败：{error}", { error: errorText(error) }),
          );
          await refreshState(true, false);
        } finally {
          importingRuntime.value = false;
        }
      },
    });
  } catch (error) {
    Message.error(
      translatedTemplate("选择版本包失败：{error}", { error: errorText(error) }),
    );
  }
}

function activateRuntime(runtimeId: string, version: string): void {
  if (runtimeId === state.value?.activeVersion) return;
  Modal.warning({
    title: translatedTemplate("切换到 v{version}", { version }),
    content: running.value
      ? t("API 服务正在运行，切换时会自动重启；如果新版本启动失败，将恢复当前版本。是否继续？")
      : t("切换后，该版本会成为下次启动 API 服务时使用的版本。是否继续？"),
    okText: t("设为当前"),
    cancelText: t("取消"),
    hideCancel: false,
    async onOk() {
      activatingRuntimeId.value = runtimeId;
      try {
        const next = await activateApiServiceRuntime(runtimeId);
        state.value = next;
        updateInfo.value = null;
        Message.success(translatedTemplate("已切换到 v{version}", { version }));
        warnRuntimeMaintenance(next);
      } catch (error) {
        Message.error(
          translatedTemplate("切换失败：{error}", { error: errorText(error) }),
        );
        await refreshState(true, false);
      } finally {
        activatingRuntimeId.value = "";
      }
    },
  });
}

function deleteRuntime(runtimeId: string, version: string): void {
  if (runtimeId === state.value?.activeVersion) return;
  Modal.warning({
    title: translatedTemplate("删除 v{version}", { version }),
    content: t("将删除这个本地运行时版本，当前使用的版本不会受影响。是否继续？"),
    okText: t("确认删除"),
    cancelText: t("取消"),
    hideCancel: false,
    async onOk() {
      deletingRuntimeId.value = runtimeId;
      try {
        const next = await deleteApiServiceRuntime(runtimeId);
        state.value = next;
        updateInfo.value = null;
        Message.success(translatedTemplate("已删除 v{version}", { version }));
        warnRuntimeMaintenance(next);
      } catch (error) {
        Message.error(
          translatedTemplate("删除失败：{error}", { error: errorText(error) }),
        );
        await refreshState(true, false);
      } finally {
        deletingRuntimeId.value = "";
      }
    },
  });
}

async function downloadUpdate(): Promise<void> {
  if (progressClearTimer) {
    window.clearTimeout(progressClearTimer);
    progressClearTimer = undefined;
  }
  downloading.value = true;
  progress.value = {
    status: "starting",
    assetName: updateInfo.value?.assetName || "",
    downloadedBytes: 0,
    totalBytes: null,
  };
  try {
    const next = await downloadApiServiceUpdate();
    state.value = next;
    if (updateInfo.value) {
      const completedUpdate = updateInfo.value;
      const activeRuntime = next.runtimes.find((runtime) => runtime.id === next.activeVersion);
      const latestInstalled = next.runtimes.some(
        (runtime) => runtime.version === completedUpdate.latestVersion,
      );
      updateInfo.value = {
        ...completedUpdate,
        currentVersion: activeRuntime?.version || completedUpdate.latestVersion,
        hasUpdate: false,
        canApply: false,
        latestInstalled,
        latestActive: activeRuntime?.version === completedUpdate.latestVersion,
      };
    }
    if (progress.value?.status !== "done") {
      progress.value = {
        status: "done",
        assetName: progress.value?.assetName || updateInfo.value?.assetName || "",
        downloadedBytes: 1,
        totalBytes: 1,
        message: next.service.running ? "API 服务已更新并重新启动" : "API 服务更新已安装",
      };
    }
    Message.success(t("API 服务更新已安装"));
    warnRuntimeMaintenance(next);
  } catch (error) {
    progress.value = {
      status: errorText(error).includes("下载已取消") ? "cancelled" : "failed",
      assetName: progress.value?.assetName || updateInfo.value?.assetName || "",
      downloadedBytes: progress.value?.downloadedBytes || 0,
      totalBytes: progress.value?.totalBytes || null,
      message: errorText(error).includes("下载已取消") ? "下载已取消" : errorText(error),
    };
    if (errorText(error).includes("下载已取消")) {
      Message.info(t("下载已取消"));
    } else {
      Message.error(`下载更新失败：${errorText(error)}`);
    }
    await refreshState(true, false);
  } finally {
    downloading.value = false;
    if (progress.value?.status === "done" || progress.value?.status === "cancelled") {
      clearProgressLater();
    }
  }
}

async function cancelDownload(): Promise<void> {
  try {
    await cancelApiServiceDownload();
  } catch (error) {
    Message.error(`取消失败：${errorText(error)}`);
  }
}

function clearProgressLater(delay = 1200): void {
  if (progressClearTimer) window.clearTimeout(progressClearTimer);
  progressClearTimer = window.setTimeout(() => {
    progress.value = null;
    progressClearTimer = undefined;
  }, delay);
}

function addApiKey(): void {
  form.apiKeys.push(generateApiKey());
}

function removeApiKey(index: number): void {
  if (form.apiKeys.length <= 1) {
    Message.warning(t("至少保留一个 API 密钥"));
    return;
  }
  form.apiKeys.splice(index, 1);
  visibleApiKeyIndexes.value = new Set();
}

function regenerateApiKeys(): void {
  form.apiKeys = generateApiKeys();
  visibleApiKeyIndexes.value = new Set();
}

function isApiKeyVisible(index: number): boolean {
  return visibleApiKeyIndexes.value.has(index);
}

function toggleApiKeyVisible(index: number): void {
  const next = new Set(visibleApiKeyIndexes.value);
  if (next.has(index)) {
    next.delete(index);
  } else {
    next.add(index);
  }
  visibleApiKeyIndexes.value = next;
}

function generateApiKeys(): string[] {
  return Array.from({ length: 3 }, () => generateApiKey());
}

function generateApiKey(): string {
  const bytes = new Uint8Array(24);
  window.crypto.getRandomValues(bytes);
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  const suffix = Array.from(bytes, (byte) => alphabet[byte % alphabet.length]).join("");
  return `sk-cpa-${suffix}`;
}

function openManagementUrl(): void {
  const url = state.value?.service.managementUrl;
  if (!url) return;
  void openExternalUrl(url).catch((error) => {
    Message.error(`打开浏览器失败：${errorText(error)}`);
  });
}

function formatBytes(bytes?: number | null): string {
  if (!bytes || !Number.isFinite(bytes)) return "--";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value >= 10 || index === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[index]}`;
}

function formatTime(seconds?: number | null): string {
  if (!seconds) return t("未检测");
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(seconds * 1000));
}

function formatCountdown(totalSeconds: number): string {
  const seconds = Math.max(0, Math.trunc(totalSeconds));
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainingSeconds = seconds % 60;
  const clock = [hours, minutes, remainingSeconds].map((value) => String(value).padStart(2, "0")).join(":");
  return days > 0 ? `${days} ${t("天")} ${clock}` : clock;
}

function errorText(error: unknown): string {
  return String(error instanceof Error ? error.message : error).replace(/^Error:\s*/, "");
}

function applyAutoUpdateEvent(event: ApiServiceAutoUpdateEvent): void {
  if (event.updateInfo) updateInfo.value = event.updateInfo;
  if (event.status === "failed") {
    downloading.value = false;
    progress.value = {
      status: "failed",
      assetName: event.updateInfo?.assetName || "CLIProxyAPI",
      downloadedBytes: progress.value?.downloadedBytes || 0,
      totalBytes: progress.value?.totalBytes || null,
      message: event.message || t("自动更新失败"),
    };
  } else if (event.status === "checked") {
    if (progress.value?.status === "failed") progress.value = null;
  } else if (event.status === "updated") {
    downloading.value = false;
    clearProgressLater();
  }
  if (panelMounted) void refreshState(true, false);
}

watch(
  () => props.autoUpdateEvent,
  (event) => {
    if (event) applyAutoUpdateEvent(event);
  },
  { immediate: true },
);

onMounted(async () => {
  panelMounted = true;
  unlistenProgress = await listen<ApiServiceDownloadProgress>(
    API_SERVICE_DOWNLOAD_PROGRESS_EVENT,
    (event) => {
      if (progressClearTimer) {
        window.clearTimeout(progressClearTimer);
        progressClearTimer = undefined;
      }
      progress.value = event.payload;
      downloading.value = ["starting", "downloading", "installing"].includes(event.payload.status);
      if (["done", "cancelled"].includes(event.payload.status)) {
        clearProgressLater();
      }
    },
  );
  countdownTimer = window.setInterval(() => {
    countdownNow.value = Date.now();
  }, 1000);
  await refreshState();
});

watch(
  () => props.active,
  (active, previous) => {
    if (active && previous === false) {
      void refreshState(true, false);
    }
  },
);

onUnmounted(() => {
  panelMounted = false;
  unlistenProgress?.();
  if (countdownTimer) window.clearInterval(countdownTimer);
  if (progressClearTimer) window.clearTimeout(progressClearTimer);
});
</script>

<template>
  <section class="api-service-panel">
    <a-spin :loading="loading" dot>
      <div class="api-service-layout">
        <div class="api-service-main">
          <a-card :bordered="false" class="api-service-hero">
            <div class="api-service-hero-top">
              <div>
                <h2>{{ t("API 服务") }}</h2>
                <p>{{ t("按需下载并运行 CLIProxyAPI，本地服务文件保存在 .codex_switcher。") }}</p>
              </div>
              <a-tag :color="running ? 'green' : serviceReady ? 'arcoblue' : 'gray'">
                {{ running ? t("运行中") : serviceReady ? t("已安装") : t("未安装") }}
              </a-tag>
            </div>

            <div class="api-service-status">
              <div>
                <span>{{ t("服务状态") }}</span>
                <strong>{{ serviceStatusText }}</strong>
              </div>
              <div>
                <span>{{ t("当前版本") }}</span>
                <strong>{{ currentRuntime?.version || t("未安装") }}</strong>
              </div>
              <div>
                <span>{{ t("访问地址") }}</span>
                <button
                  class="api-service-url-button"
                  type="button"
                  :disabled="!running"
                  :title="state?.service.managementUrl || 'http://127.0.0.1:17877/management.html'"
                  @click="openManagementUrl"
                >
                  <strong>{{ state?.service.managementUrl || "http://127.0.0.1:17877/management.html" }}</strong>
                  <icon-link />
                </button>
              </div>
            </div>

            <div v-if="progress" class="api-service-progress">
              <div>
                <strong>{{ progress.message ? t(progress.message) : downloading ? t("正在处理服务包") : t("下载状态") }}</strong>
                <span>
                  {{ progress.assetName || "CLIProxyAPI" }}
                  · {{ progressDetail }}
                </span>
              </div>
              <a-progress :percent="progressPercent" :status="progressStatus" />
              <a-button v-if="downloading" size="small" @click="cancelDownload">{{ t("取消下载") }}</a-button>
            </div>

            <div class="api-service-actions">
              <a-button
                type="primary"
                status="normal"
                :loading="addingAccount"
                :disabled="!serviceReady || !firstApiKey"
                @click="addToAccountOverview"
              >
                <template #icon><icon-plus /></template>
                {{ t("添加到账号总览") }}
              </a-button>
              <a-button
                v-if="!running"
                type="primary"
                :loading="starting || downloading"
                @click="start"
              >
                <template #icon><icon-play-arrow /></template>
                {{ serviceReady ? t("开启服务") : t("下载并开启") }}
              </a-button>
              <a-button v-else status="danger" :loading="stopping" @click="stop">
                <template #icon><icon-pause /></template>
                {{ t("停止服务") }}
              </a-button>
              <a-button :loading="checking" @click="checkUpdate">
                <template #icon><icon-refresh /></template>
                {{ t("检测更新") }}
              </a-button>
              <a-button
                type="primary"
                status="success"
                :disabled="!canDownloadUpdate"
                :loading="downloading"
                @click="downloadUpdate"
              >
                <template #icon><icon-download /></template>
                {{ t("下载更新") }}
              </a-button>
              <a-button
                :disabled="downloading || starting || stopping || runtimeOperationBusy"
                @click="openVersionManager"
              >
                <template #icon><icon-list /></template>
                {{ t("版本管理") }}
              </a-button>
              <a-button
                html-type="button"
                status="danger"
                :loading="resetting"
                :disabled="downloading || starting || stopping"
                @click.stop="resetService"
              >
                <template #icon><icon-delete /></template>
                {{ t("重置服务") }}
              </a-button>
              <a-button :disabled="!serviceReady" @click="openBindAccounts">
                <template #icon><icon-link /></template>
                {{ t("绑定账号") }}
              </a-button>
              <a-button :disabled="!serviceReady" @click="openDeleteAccounts">
                <template #icon><icon-delete /></template>
                {{ t("删除账号") }}
              </a-button>
            </div>
          </a-card>

          <a-card v-if="serviceReady" :title="t('服务配置')" :bordered="false" class="api-service-card">
            <a-form :model="form" layout="vertical">
              <div class="api-service-form-grid">
                <a-form-item :label="t('端口')">
                  <a-input-number v-model="form.port" :min="1" :max="65535" mode="button" />
                </a-form-item>
                <a-form-item :label="t('管理密钥')">
                  <a-input-password v-model="form.managementKey" allow-clear />
                </a-form-item>
                <a-form-item :label="t('自动更新')">
                  <div class="api-service-auto-update-control">
                    <a-switch v-model="form.autoUpdate" />
                    <span :class="{ pending: autoUpdateFormChanged }">
                      <icon-clock-circle />
                      {{ autoUpdateCountdown }}
                    </span>
                  </div>
                </a-form-item>
                <a-form-item :label="t('检测间隔')">
                  <a-input-number
                    v-model="form.autoUpdateIntervalHours"
                    :min="1"
                    :max="720"
                    mode="button"
                  >
                    <template #suffix>{{ t("小时") }}</template>
                  </a-input-number>
                </a-form-item>
              </div>
              <div class="api-service-key-head">
                <div>
                  <strong>{{ t("API 密钥") }}</strong>
                  <span>{{ t("默认使用第一个密钥添加账号，调用地址：") }}{{ apiBaseUrl }}</span>
                </div>
                <div>
                  <a-button size="small" @click="regenerateApiKeys">
                    <template #icon><icon-refresh /></template>
                    {{ t("随机重生成") }}
                  </a-button>
                  <a-button size="small" type="primary" @click="addApiKey">
                    <template #icon><icon-plus /></template>
                    {{ t("添加密钥") }}
                  </a-button>
                </div>
              </div>
              <div class="api-service-key-list">
                <div v-for="(_, index) in form.apiKeys" :key="index" class="api-service-key-row">
                  <span>{{ index + 1 }}</span>
                  <a-input
                    v-model="form.apiKeys[index]"
                    class="api-service-key-input"
                    :type="isApiKeyVisible(index) ? 'text' : 'password'"
                    :placeholder="t('请输入 API 密钥')"
                  />
                  <a-button
                    class="api-service-key-visibility"
                    html-type="button"
                    @click.stop="toggleApiKeyVisible(index)"
                  >
                    <template #icon>
                      <icon-eye-invisible v-if="isApiKeyVisible(index)" />
                      <icon-eye v-else />
                    </template>
                  </a-button>
                  <a-button
                    class="api-service-key-delete"
                    html-type="button"
                    status="danger"
                    :disabled="form.apiKeys.length <= 1"
                    @click="removeApiKey(index)"
                  >
                    <template #icon><icon-delete /></template>
                  </a-button>
                </div>
              </div>
              <a-button type="primary" :loading="saving" :disabled="running" @click="saveSettings">
                {{ t("保存配置") }}
              </a-button>
              <p v-if="running" class="api-service-note">{{ t("服务运行中不能修改端口或密钥，请先停止服务。") }}</p>
            </a-form>
          </a-card>
        </div>

        <aside class="api-service-side">
          <a-card :title="t('更新信息')" :bordered="false" class="api-service-card">
            <div class="api-service-info-list">
              <div>
                <span>{{ t("最新版本") }}</span>
                <strong>{{ updateInfo?.latestVersion || t("未检测") }}</strong>
              </div>
              <div>
                <span>{{ t("匹配平台") }}</span>
                <strong>{{ updateInfo?.target || currentRuntime?.target || "--" }}</strong>
              </div>
              <div>
                <span>{{ t("上次检测") }}</span>
                <strong>{{ formatTime(state?.settings.lastUpdateCheckAt) }}</strong>
              </div>
            </div>
          </a-card>

          <a-card :title="t('本地目录')" :bordered="false" class="api-service-card">
            <div class="api-service-paths">
              <div>
                <span>{{ t("服务目录") }}</span>
                <code>{{ state?.baseDir || "~/.codex_switcher/api-service" }}</code>
              </div>
              <div>
                <span>{{ t("运行时") }}</span>
                <code>{{ state?.runtimeDir || "--" }}</code>
              </div>
              <div>
                <span>{{ t("工作区") }}</span>
                <code>{{ state?.workspaceDir || "--" }}</code>
              </div>
              <div>
                <span>{{ t("配置文件") }}</span>
                <code>{{ state?.configPath || "--" }}</code>
              </div>
              <div>
                <span>{{ t("认证目录") }}</span>
                <code>{{ state?.authDir || "--" }}</code>
              </div>
            </div>
          </a-card>
        </aside>
      </div>
    </a-spin>

    <a-modal
      v-model:visible="versionVisible"
      :title="t('API 服务版本管理')"
      width="980px"
      :footer="false"
      :mask-closable="!runtimeOperationBusy"
      :closable="!runtimeOperationBusy"
      :esc-to-close="!runtimeOperationBusy"
    >
      <div class="api-service-version-manager">
        <div class="api-service-version-toolbar">
          <div>
            <strong>{{ t("本地版本") }}</strong>
            <p>
              {{ t("更新或导入成功后默认切换到本机最高版本，并保留当前版本和最多 2 个旧版本。") }}
            </p>
          </div>
          <a-button
            type="primary"
            :loading="importingRuntime"
            :disabled="Boolean(activatingRuntimeId || deletingRuntimeId || downloading)"
            @click="chooseRuntimePackage"
          >
            <template #icon><icon-upload /></template>
            {{ t("导入本地包") }}
          </a-button>
        </div>

        <a-alert v-if="runtimeMaintenanceMessage(state)" type="warning" show-icon>
          {{ runtimeMaintenanceMessage(state) }}
        </a-alert>

        <div v-if="state?.runtimes.length" class="api-service-version-table">
          <div class="api-service-version-row api-service-version-header" aria-hidden="true">
            <span>{{ t("版本") }}</span>
            <span>{{ t("平台") }}</span>
            <span>{{ t("导入时间") }}</span>
            <span>{{ t("包文件") }}</span>
            <span>{{ t("状态与操作") }}</span>
          </div>
          <div
            v-for="runtime in state.runtimes"
            :key="runtime.id"
            class="api-service-version-row"
            :class="{ current: runtime.id === state.activeVersion }"
          >
            <div :data-label="t('版本')">
              <strong>v{{ runtime.version }}</strong>
            </div>
            <div :data-label="t('平台')">
              <code>{{ runtime.target }}</code>
            </div>
            <div :data-label="t('导入时间')">
              <span>{{ formatTime(runtime.installedAt) }}</span>
            </div>
            <div :data-label="t('包文件')" class="api-service-version-package" :title="runtime.packageFile">
              <span>{{ runtime.packageFile }}</span>
            </div>
            <div :data-label="t('状态与操作')" class="api-service-version-operations">
              <a-tag v-if="runtime.id === state.activeVersion" color="green">
                <template #icon><icon-check-circle /></template>
                {{ t("当前") }}
              </a-tag>
              <a-tag v-else-if="!runtime.compatible" color="gray">
                {{ t("平台不兼容") }}
              </a-tag>
              <a-button
                v-else
                size="small"
                :loading="activatingRuntimeId === runtime.id"
                :disabled="runtimeOperationBusy || downloading"
                @click="activateRuntime(runtime.id, runtime.version)"
              >
                {{ t("设为当前") }}
              </a-button>
              <a-button
                size="small"
                status="danger"
                :loading="deletingRuntimeId === runtime.id"
                :disabled="runtime.id === state.activeVersion || runtimeOperationBusy || downloading"
                @click="deleteRuntime(runtime.id, runtime.version)"
              >
                <template #icon><icon-delete /></template>
                {{ runtime.id === state.activeVersion ? t("不可删除") : t("删除") }}
              </a-button>
            </div>
          </div>
        </div>
        <a-empty v-else :description="t('暂无本地版本，可导入 CLIProxyAPI 官方安装包')" />

        <div class="api-service-import-warning">
          <icon-info-circle />
          <span>{{ t("手动导入不会联网校验 GitHub checksums，请只选择可信的官方安装包。") }}</span>
        </div>
      </div>
    </a-modal>

    <a-modal
      v-model:visible="bindVisible"
      :title="t('绑定账号到 API 服务')"
      width="760px"
      :footer="false"
    >
      <div class="api-service-account-modal">
        <p>{{ t("OAuth 账号会写入认证目录，API Key 账号会写入 CLIProxyAPI 上游配置。") }}</p>
        <a-input
          v-model="bindSearchKeyword"
          allow-clear
          :placeholder="t('筛选邮箱 / 昵称')"
        >
          <template #prefix><icon-search /></template>
        </a-input>
        <div v-if="bindableAccounts.length" class="api-service-account-select-all">
          <a-checkbox
            :model-value="allBindSelected"
            :indeterminate="bindSelectionIndeterminate"
            @change="(checked) => toggleAllBindAccounts(Boolean(checked))"
          >
            {{ t("全选") }}
          </a-checkbox>
          <span>
            {{ t("已选") }} {{ selectedBindCount }} / {{ bindableAccounts.length }}
            <template v-if="bindSearchKeyword.trim()">· {{ filteredBindableAccounts.length }} {{ t("条") }}</template>
          </span>
        </div>
        <div class="api-service-account-list">
          <label v-for="account in filteredBindableAccounts" :key="account.id" class="api-service-account-row">
            <a-checkbox
              :model-value="selectedBindIds.has(account.id)"
              @change="(checked) => toggleBindAccount(account.id, Boolean(checked))"
            />
            <div class="api-service-account-main">
              <strong>{{ accountDisplayName(account) }}</strong>
              <span v-if="isApiKeyAccount(account)">
                API Key · {{ account.api_provider_name || account.apiProviderName || t("自定义服务") }} ·
                {{ account.api_base_url || account.apiBaseUrl || "https://api.openai.com/v1" }}
              </span>
              <span v-else>OAuth · {{ account.email || account.id }}</span>
              <div v-if="account.quota && hasAnyQuotaWindow(account.quota)" class="api-service-account-quota">
                <div v-if="hasQuotaWindow(account.quota, 'hourly')" class="api-service-quota-line">
                  <span>
                    <icon-calendar v-if="isFreePlanAccount(account)" />
                    <icon-clock-circle v-else />
                    {{ isFreePlanAccount(account) ? t("长周期") : t("短周期") }}
                  </span>
                  <strong :style="{ color: quotaColor(account.quota.hourly_percentage) }">
                    {{ account.quota.hourly_percentage }}%
                  </strong>
                  <small>{{ quotaWindowLabel(account.quota.hourly_window_minutes, t('5 小时窗口')) }}</small>
                  <em>{{ quotaResetLeftLabel(account.quota.hourly_reset_time) }}</em>
                </div>
                <div
                  v-if="!isFreePlanAccount(account) && hasQuotaWindow(account.quota, 'weekly')"
                  class="api-service-quota-line"
                >
                  <span><icon-calendar /> {{ t("长周期") }}</span>
                  <strong :style="{ color: quotaColor(account.quota.weekly_percentage) }">
                    {{ account.quota.weekly_percentage }}%
                  </strong>
                  <small>{{ quotaWindowLabel(account.quota.weekly_window_minutes, t('7 天窗口')) }}</small>
                  <em>{{ quotaResetLeftLabel(account.quota.weekly_reset_time) }}</em>
                </div>
              </div>
              <div v-else-if="account.quota_error" class="api-service-account-quota-error">
                {{ account.quota_error.message }}
              </div>
            </div>
            <PlanBadge :label="planLabel(account)" :badge-class="planClass(account)" />
          </label>
          <a-empty
            v-if="!filteredBindableAccounts.length"
            :description="t(bindableAccounts.length ? '没有匹配的账号' : '暂无可绑定账号')"
          />
        </div>
        <div class="api-service-modal-actions">
          <a-button @click="bindVisible = false">{{ t("取消") }}</a-button>
          <a-button type="primary" :loading="bindingAccounts" @click="bindSelectedAccounts">
            {{ t("确认绑定") }}
          </a-button>
        </div>
      </div>
    </a-modal>

    <a-modal
      v-model:visible="deleteVisible"
      :title="t('删除 API 服务账号')"
      width="760px"
      :footer="false"
    >
      <div class="api-service-account-modal">
        <p>{{ t("删除会移除对应 OAuth 认证文件或由本应用管理的 API Key 上游配置。") }}</p>
        <div v-if="boundAccounts.length" class="api-service-account-select-all">
          <a-checkbox
            :model-value="allDeleteSelected"
            :indeterminate="deleteSelectionIndeterminate"
            @change="(checked) => toggleAllDeleteAccounts(Boolean(checked))"
          >
            {{ t("全选") }}
          </a-checkbox>
          <span>{{ t("已选") }} {{ selectedDeleteCount }} / {{ boundAccounts.length }}</span>
        </div>
        <div class="api-service-account-list">
          <label v-for="account in boundAccounts" :key="account.id" class="api-service-account-row">
            <a-checkbox
              :model-value="selectedDeleteIds.has(account.id)"
              @change="(checked) => toggleDeleteAccount(account.id, Boolean(checked))"
            />
            <div class="api-service-account-main">
              <strong>{{ account.label }}</strong>
              <span v-if="account.kind === 'apikey'">API Key · {{ account.baseUrl }}</span>
              <span v-else>{{ t("CPA 认证账号") }} · {{ account.email }}</span>
              <template v-if="boundSourceAccount(account)?.quota && hasAnyQuotaWindow(boundSourceAccount(account)?.quota)">
                <div class="api-service-account-quota">
                  <div
                    v-if="hasQuotaWindow(boundSourceAccount(account)?.quota, 'hourly')"
                    class="api-service-quota-line"
                  >
                    <span>
                      <icon-calendar v-if="isFreePlanAccount(boundSourceAccount(account)!)" />
                      <icon-clock-circle v-else />
                      {{ isFreePlanAccount(boundSourceAccount(account)!) ? t("长周期") : t("短周期") }}
                    </span>
                    <strong :style="{ color: quotaColor(boundSourceAccount(account)!.quota!.hourly_percentage) }">
                      {{ boundSourceAccount(account)!.quota!.hourly_percentage }}%
                    </strong>
                    <small>{{ quotaWindowLabel(boundSourceAccount(account)!.quota!.hourly_window_minutes, t('5 小时窗口')) }}</small>
                    <em>{{ quotaResetLeftLabel(boundSourceAccount(account)!.quota!.hourly_reset_time) }}</em>
                  </div>
                  <div
                    v-if="!isFreePlanAccount(boundSourceAccount(account)!) && hasQuotaWindow(boundSourceAccount(account)?.quota, 'weekly')"
                    class="api-service-quota-line"
                  >
                    <span><icon-calendar /> {{ t("长周期") }}</span>
                    <strong :style="{ color: quotaColor(boundSourceAccount(account)!.quota!.weekly_percentage) }">
                      {{ boundSourceAccount(account)!.quota!.weekly_percentage }}%
                    </strong>
                    <small>{{ quotaWindowLabel(boundSourceAccount(account)!.quota!.weekly_window_minutes, t('7 天窗口')) }}</small>
                    <em>{{ quotaResetLeftLabel(boundSourceAccount(account)!.quota!.weekly_reset_time) }}</em>
                  </div>
                </div>
              </template>
              <div v-else-if="boundSourceAccount(account)?.quota_error" class="api-service-account-quota-error">
                {{ boundSourceAccount(account)?.quota_error?.message }}
              </div>
            </div>
            <PlanBadge :label="boundPlanLabel(account)" :badge-class="boundPlanClass(account)" />
          </label>
          <a-empty v-if="!boundAccounts.length" :description="t('认证目录里暂无账号')" />
        </div>
        <div class="api-service-modal-actions">
          <a-button @click="deleteVisible = false">{{ t("取消") }}</a-button>
          <a-button status="danger" :loading="deletingBoundAccounts" @click="deleteSelectedBoundAccounts">
            {{ t("确认删除") }}
          </a-button>
        </div>
      </div>
    </a-modal>
  </section>
</template>
