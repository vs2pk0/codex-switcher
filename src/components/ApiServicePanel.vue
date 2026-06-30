<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { addCodexAccountWithApiKey, openExternalUrl, type CodexSwitcherSettings } from "../services/codex";
import type { CodexAccount } from "../types/codex";
import PlanBadge from "./PlanBadge.vue";
import {
  API_SERVICE_DOWNLOAD_PROGRESS_EVENT,
  bindApiServiceAccounts,
  cancelApiServiceDownload,
  checkApiServiceUpdate,
  deleteApiServiceBoundAccounts,
  downloadApiServiceUpdate,
  getApiServiceState,
  listApiServiceBoundAccounts,
  resetApiService,
  startApiService,
  stopApiService,
  updateApiServiceSettings,
  type ApiServiceBoundAccount,
  type ApiServiceDownloadProgress,
  type ApiServiceState,
  type ApiServiceUpdateInfo,
} from "../services/apiService";

const props = defineProps<{
  accounts: CodexAccount[];
  settings: CodexSwitcherSettings;
}>();

const emit = defineEmits<{
  (event: "account-added"): void;
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
const selectedBindIds = ref<Set<string>>(new Set());
const selectedDeleteEmails = ref<Set<string>>(new Set());
const boundAccounts = ref<ApiServiceBoundAccount[]>([]);
const progress = ref<ApiServiceDownloadProgress | null>(null);
let unlistenProgress: UnlistenFn | null = null;
let autoUpdateTimer: number | undefined;
let progressClearTimer: number | undefined;

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
const canDownloadUpdate = computed(() =>
  Boolean(updateInfo.value?.downloadUrl && (updateInfo.value.hasUpdate || !updateInfo.value.latestInstalled)),
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
const progressDetail = computed(() => {
  if (!progress.value) return "";
  const total = progress.value.totalBytes || 0;
  const downloaded = progress.value.downloadedBytes || 0;
  if (progress.value.status === "installing") return "安装中";
  if (progress.value.status === "done") return "100%";
  if (progress.value.status === "failed") return "失败";
  if (progress.value.status === "cancelled") return "已取消";
  if (!total) return downloaded > 0 ? `${formatBytes(downloaded)} · 正在下载` : "准备中";
  return `${formatBytes(downloaded)} / ${formatBytes(total)}`;
});
const serviceStatusText = computed(() => {
  if (running.value) return `运行中 · PID ${state.value?.service.pid || "--"}`;
  if (serviceReady.value) return "已安装，当前未启动";
  return "未安装，首次开启时会下载服务";
});
const apiBaseUrl = computed(() => `http://127.0.0.1:${form.port || 17877}/v1`);
const firstApiKey = computed(() => form.apiKeys.find((key) => key.trim())?.trim() || "");
const oauthAccounts = computed(() => props.accounts.filter((account) => !isApiKeyAccount(account)));
const selectedBindCount = computed(
  () => oauthAccounts.value.filter((account) => selectedBindIds.value.has(account.id)).length,
);
const allBindSelected = computed(
  () => oauthAccounts.value.length > 0 && selectedBindCount.value === oauthAccounts.value.length,
);
const bindSelectionIndeterminate = computed(
  () => selectedBindCount.value > 0 && selectedBindCount.value < oauthAccounts.value.length,
);
const selectedDeleteCount = computed(
  () => boundAccounts.value.filter((account) => selectedDeleteEmails.value.has(account.email)).length,
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

async function refreshState(silent = false): Promise<void> {
  loading.value = !silent;
  try {
    const next = await getApiServiceState();
    state.value = next;
    syncForm(next);
    scheduleAutoUpdate();
  } catch (error) {
    if (!silent) Message.error(`加载 API 服务失败：${errorText(error)}`);
  } finally {
    loading.value = false;
  }
}

function scheduleAutoUpdate(): void {
  if (autoUpdateTimer) {
    window.clearTimeout(autoUpdateTimer);
    autoUpdateTimer = undefined;
  }
  const current = state.value;
  if (!current?.installed || !current.settings.autoUpdate) return;
  const intervalMs = Math.max(1, current.settings.autoUpdateIntervalHours || 24) * 60 * 60 * 1000;
  const last = (current.settings.lastUpdateCheckAt || 0) * 1000;
  const nextAt = last ? last + intervalMs : Date.now() + 5000;
  const delay = Math.max(5000, nextAt - Date.now());
  autoUpdateTimer = window.setTimeout(() => {
    void runAutoUpdate();
  }, delay);
}

async function runAutoUpdate(): Promise<void> {
  if (!state.value?.installed || !state.value.settings.autoUpdate || checking.value || downloading.value) {
    scheduleAutoUpdate();
    return;
  }
  checking.value = true;
  try {
    const nextUpdate = await checkApiServiceUpdate();
    updateInfo.value = nextUpdate;
    await refreshState(true);
    if (nextUpdate.downloadUrl && (nextUpdate.hasUpdate || !nextUpdate.latestInstalled)) {
      await downloadUpdate();
    }
  } catch {
    scheduleAutoUpdate();
  } finally {
    checking.value = false;
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
    scheduleAutoUpdate();
    Message.success("API 服务配置已保存");
  } catch (error) {
    Message.error(`保存失败：${errorText(error)}`);
  } finally {
    saving.value = false;
  }
}

async function addToAccountOverview(): Promise<void> {
  if (!serviceReady.value) {
    Message.warning("请先下载并开启 API 服务");
    return;
  }
  const apiKey = firstApiKey.value;
  if (!apiKey) {
    Message.error("请先添加一个 API 密钥");
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
    emit("account-added");
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
    scheduleAutoUpdate();
    progress.value = null;
    Message.success("API 服务已开启");
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
    scheduleAutoUpdate();
    Message.success("API 服务已停止");
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
  const base = planDisplayName(account.plan_type);
  if (base !== "PRO") return base;
  const authPlan = normalizeAuthFilePlan(account.auth_file_plan_type || account.plan_type);
  return authPlan === "prolite" ? "PRO 5X" : "PRO 20X";
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

function planClass(account: CodexAccount): string {
  const badgeKey = badgeTypeKey(account);
  const styleName =
    props.settings.badgeStyles?.[badgeKey] ||
    props.settings.badgeStyle ||
    "classic";
  const style = `badge-${styleName}`;
  if (isApiKeyAccount(account)) return `api ${style}`;
  const key = normalizePlanKey(account.plan_type);
  if (key === "pro") {
    const proClass =
      normalizeAuthFilePlan(account.auth_file_plan_type || account.plan_type) === "prolite"
        ? "pro-lite"
        : "pro-max";
    return `${proClass} ${style}`;
  }
  return `${key} ${style}`;
}

function normalizedEmail(value?: string): string {
  return (value || "").trim().toLowerCase();
}

function boundSourceAccount(account: ApiServiceBoundAccount): CodexAccount | undefined {
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
  return !isApiKeyAccount(account) && normalizePlanKey(account.plan_type) === "free";
}

function quotaWindowLabel(minutes?: number, fallback = "5 小时窗口"): string {
  if (!minutes || !Number.isFinite(minutes)) return fallback;
  if (minutes % (60 * 24) === 0) return `${minutes / 60 / 24} 天窗口`;
  if (minutes % 60 === 0) return `${minutes / 60} 小时窗口`;
  return `${minutes} 分钟窗口`;
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
  if (!date) return "等待刷新";
  const pad = (input: number) => String(input).padStart(2, "0");
  return `更新 ${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`;
}

function openBindAccounts(): void {
  selectedBindIds.value = new Set(oauthAccounts.value.map((account) => account.id));
  bindVisible.value = true;
}

async function bindSelectedAccounts(): Promise<void> {
  const ids = [...selectedBindIds.value];
  if (!ids.length) {
    Message.warning("请选择要绑定的 OAuth 账号");
    return;
  }
  bindingAccounts.value = true;
  try {
    const summary = await bindApiServiceAccounts(ids);
    bindVisible.value = false;
    Message.success(`已绑定 ${summary.count} 个账号到 API 服务`);
    await loadBoundAccounts();
  } catch (error) {
    Message.error(`绑定失败：${errorText(error)}`);
  } finally {
    bindingAccounts.value = false;
  }
}

async function openDeleteAccounts(): Promise<void> {
  await loadBoundAccounts();
  selectedDeleteEmails.value = new Set(boundAccounts.value.map((account) => account.email));
  deleteVisible.value = true;
}

async function loadBoundAccounts(): Promise<void> {
  try {
    boundAccounts.value = await listApiServiceBoundAccounts();
  } catch (error) {
    Message.error(`读取 API 服务账号失败：${errorText(error)}`);
  }
}

async function deleteSelectedBoundAccounts(): Promise<void> {
  const emails = [...selectedDeleteEmails.value];
  if (!emails.length) {
    Message.warning("请选择要删除的 API 服务账号");
    return;
  }
  deletingBoundAccounts.value = true;
  try {
    const summary = await deleteApiServiceBoundAccounts(emails);
    await loadBoundAccounts();
    selectedDeleteEmails.value = new Set();
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
  selectedBindIds.value = checked
    ? new Set(oauthAccounts.value.map((account) => account.id))
    : new Set();
}

function toggleDeleteAccount(email: string, checked: boolean): void {
  const next = new Set(selectedDeleteEmails.value);
  if (checked) next.add(email);
  else next.delete(email);
  selectedDeleteEmails.value = next;
}

function toggleAllDeleteAccounts(checked: boolean): void {
  selectedDeleteEmails.value = checked
    ? new Set(boundAccounts.value.map((account) => account.email))
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
        scheduleAutoUpdate();
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
    if (updateInfo.value.hasUpdate) {
      Message.info(`发现新版本 ${updateInfo.value.latestVersion}`);
    } else {
      Message.success(`当前已是最新版本 ${updateInfo.value.latestVersion}`);
    }
    await refreshState(true);
  } catch (error) {
    Message.error(`检测更新失败：${errorText(error)}`);
  } finally {
    checking.value = false;
  }
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
    syncForm(next);
    scheduleAutoUpdate();
    updateInfo.value = await checkApiServiceUpdate().catch(() => updateInfo.value);
    if (progress.value?.status !== "done") {
      progress.value = {
        status: "done",
        assetName: progress.value?.assetName || updateInfo.value?.assetName || "",
        downloadedBytes: 1,
        totalBytes: 1,
        message: next.service.running ? "API 服务已更新并重新启动" : "API 服务更新已安装",
      };
    }
    Message.success("API 服务更新已安装");
  } catch (error) {
    progress.value = {
      status: errorText(error).includes("下载已取消") ? "cancelled" : "failed",
      assetName: progress.value?.assetName || updateInfo.value?.assetName || "",
      downloadedBytes: progress.value?.downloadedBytes || 0,
      totalBytes: progress.value?.totalBytes || null,
      message: errorText(error).includes("下载已取消") ? "下载已取消" : errorText(error),
    };
    if (errorText(error).includes("下载已取消")) {
      Message.info("下载已取消");
    } else {
      Message.error(`下载更新失败：${errorText(error)}`);
    }
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
    Message.warning("至少保留一个 API 密钥");
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
  if (!seconds) return "未检测";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(seconds * 1000));
}

function errorText(error: unknown): string {
  return String(error instanceof Error ? error.message : error).replace(/^Error:\s*/, "");
}

onMounted(async () => {
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
  await refreshState();
});

onUnmounted(() => {
  unlistenProgress?.();
  if (autoUpdateTimer) window.clearTimeout(autoUpdateTimer);
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
                <h2>API 服务</h2>
                <p>按需下载并运行 CLIProxyAPI，本地服务文件保存在 .codex_switcher。</p>
              </div>
              <a-tag :color="running ? 'green' : serviceReady ? 'arcoblue' : 'gray'">
                {{ running ? "运行中" : serviceReady ? "已安装" : "未安装" }}
              </a-tag>
            </div>

            <div class="api-service-status">
              <div>
                <span>服务状态</span>
                <strong>{{ serviceStatusText }}</strong>
              </div>
              <div>
                <span>当前版本</span>
                <strong>{{ currentRuntime?.version || "未安装" }}</strong>
              </div>
              <div>
                <span>访问地址</span>
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
                <strong>{{ progress.message || (downloading ? "正在处理服务包" : "下载状态") }}</strong>
                <span>
                  {{ progress.assetName || "CLIProxyAPI" }}
                  · {{ progressDetail }}
                </span>
              </div>
              <a-progress :percent="progressPercent" :status="progressStatus" />
              <a-button v-if="downloading" size="small" @click="cancelDownload">取消下载</a-button>
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
                添加到账号总览
              </a-button>
              <a-button
                v-if="!running"
                type="primary"
                :loading="starting || downloading"
                @click="start"
              >
                <template #icon><icon-play-arrow /></template>
                {{ serviceReady ? "开启服务" : "下载并开启" }}
              </a-button>
              <a-button v-else status="danger" :loading="stopping" @click="stop">
                <template #icon><icon-pause /></template>
                停止服务
              </a-button>
              <a-button :loading="checking" @click="checkUpdate">
                <template #icon><icon-refresh /></template>
                检测更新
              </a-button>
              <a-button
                type="primary"
                status="success"
                :disabled="!canDownloadUpdate"
                :loading="downloading"
                @click="downloadUpdate"
              >
                <template #icon><icon-download /></template>
                下载更新
              </a-button>
              <a-button
                html-type="button"
                status="danger"
                :loading="resetting"
                :disabled="downloading || starting || stopping"
                @click.stop="resetService"
              >
                <template #icon><icon-delete /></template>
                重置服务
              </a-button>
              <a-button :disabled="!serviceReady" @click="openBindAccounts">
                <template #icon><icon-link /></template>
                绑定账号
              </a-button>
              <a-button :disabled="!serviceReady" @click="openDeleteAccounts">
                <template #icon><icon-delete /></template>
                删除账号
              </a-button>
            </div>
          </a-card>

          <a-card v-if="serviceReady" title="服务配置" :bordered="false" class="api-service-card">
            <a-form :model="form" layout="vertical">
              <div class="api-service-form-grid">
                <a-form-item label="端口">
                  <a-input-number v-model="form.port" :min="1" :max="65535" mode="button" />
                </a-form-item>
                <a-form-item label="管理密钥">
                  <a-input-password v-model="form.managementKey" allow-clear />
                </a-form-item>
                <a-form-item label="自动更新">
                  <a-switch v-model="form.autoUpdate" />
                </a-form-item>
                <a-form-item label="检测间隔">
                  <a-input-number
                    v-model="form.autoUpdateIntervalHours"
                    :min="1"
                    :max="720"
                    mode="button"
                  >
                    <template #suffix>小时</template>
                  </a-input-number>
                </a-form-item>
              </div>
              <div class="api-service-key-head">
                <div>
                  <strong>API 密钥</strong>
                  <span>默认使用第一个密钥添加账号，调用地址：{{ apiBaseUrl }}</span>
                </div>
                <div>
                  <a-button size="small" @click="regenerateApiKeys">
                    <template #icon><icon-refresh /></template>
                    随机重生成
                  </a-button>
                  <a-button size="small" type="primary" @click="addApiKey">
                    <template #icon><icon-plus /></template>
                    添加密钥
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
                    placeholder="请输入 API 密钥"
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
                保存配置
              </a-button>
              <p v-if="running" class="api-service-note">服务运行中不能修改端口或密钥，请先停止服务。</p>
            </a-form>
          </a-card>
        </div>

        <aside class="api-service-side">
          <a-card title="更新信息" :bordered="false" class="api-service-card">
            <div class="api-service-info-list">
              <div>
                <span>最新版本</span>
                <strong>{{ updateInfo?.latestVersion || "未检测" }}</strong>
              </div>
              <div>
                <span>匹配平台</span>
                <strong>{{ updateInfo?.target || currentRuntime?.target || "--" }}</strong>
              </div>
              <div>
                <span>上次检测</span>
                <strong>{{ formatTime(state?.settings.lastUpdateCheckAt) }}</strong>
              </div>
            </div>
          </a-card>

          <a-card title="本地目录" :bordered="false" class="api-service-card">
            <div class="api-service-paths">
              <div>
                <span>服务目录</span>
                <code>{{ state?.baseDir || "~/.codex_switcher/api-service" }}</code>
              </div>
              <div>
                <span>运行时</span>
                <code>{{ state?.runtimeDir || "--" }}</code>
              </div>
              <div>
                <span>工作区</span>
                <code>{{ state?.workspaceDir || "--" }}</code>
              </div>
              <div>
                <span>配置文件</span>
                <code>{{ state?.configPath || "--" }}</code>
              </div>
              <div>
                <span>认证目录</span>
                <code>{{ state?.authDir || "--" }}</code>
              </div>
            </div>
          </a-card>
        </aside>
      </div>
    </a-spin>

    <a-modal
      v-model:visible="bindVisible"
      title="绑定账号到 API 服务"
      width="760px"
      :footer="false"
    >
      <div class="api-service-account-modal">
        <p>选择 OAuth 账号后会转换为 CPA 格式，并写入 API 服务的认证目录。</p>
        <div v-if="oauthAccounts.length" class="api-service-account-select-all">
          <a-checkbox
            :model-value="allBindSelected"
            :indeterminate="bindSelectionIndeterminate"
            @change="(checked) => toggleAllBindAccounts(Boolean(checked))"
          >
            全选
          </a-checkbox>
          <span>已选 {{ selectedBindCount }} / {{ oauthAccounts.length }}</span>
        </div>
        <div class="api-service-account-list">
          <label v-for="account in oauthAccounts" :key="account.id" class="api-service-account-row">
            <a-checkbox
              :model-value="selectedBindIds.has(account.id)"
              @change="(checked) => toggleBindAccount(account.id, Boolean(checked))"
            />
            <div class="api-service-account-main">
              <strong>{{ accountDisplayName(account) }}</strong>
              <span>OAuth · {{ account.email || account.id }}</span>
              <div v-if="account.quota" class="api-service-account-quota">
                <div v-if="account.quota.hourly_window_present !== false" class="api-service-quota-line">
                  <span>
                    <icon-calendar v-if="isFreePlanAccount(account)" />
                    <icon-clock-circle v-else />
                    {{ isFreePlanAccount(account) ? "长周期" : "短周期" }}
                  </span>
                  <strong :style="{ color: quotaColor(account.quota.hourly_percentage) }">
                    {{ account.quota.hourly_percentage }}%
                  </strong>
                  <small>{{ quotaWindowLabel(account.quota.hourly_window_minutes, '5 小时窗口') }}</small>
                  <em>{{ quotaResetLeftLabel(account.quota.hourly_reset_time) }}</em>
                </div>
                <div
                  v-if="!isFreePlanAccount(account) && account.quota.weekly_window_present !== false"
                  class="api-service-quota-line"
                >
                  <span><icon-calendar /> 长周期</span>
                  <strong :style="{ color: quotaColor(account.quota.weekly_percentage) }">
                    {{ account.quota.weekly_percentage }}%
                  </strong>
                  <small>{{ quotaWindowLabel(account.quota.weekly_window_minutes, '7 天窗口') }}</small>
                  <em>{{ quotaResetLeftLabel(account.quota.weekly_reset_time) }}</em>
                </div>
              </div>
              <div v-else-if="account.quota_error" class="api-service-account-quota-error">
                {{ account.quota_error.message }}
              </div>
            </div>
            <PlanBadge :label="planLabel(account)" :badge-class="planClass(account)" />
          </label>
          <a-empty v-if="!oauthAccounts.length" description="暂无可绑定的 OAuth 账号" />
        </div>
        <div class="api-service-modal-actions">
          <a-button @click="bindVisible = false">取消</a-button>
          <a-button type="primary" :loading="bindingAccounts" @click="bindSelectedAccounts">
            确认绑定
          </a-button>
        </div>
      </div>
    </a-modal>

    <a-modal
      v-model:visible="deleteVisible"
      title="删除 API 服务账号"
      width="760px"
      :footer="false"
    >
      <div class="api-service-account-modal">
        <p>这里从认证目录 JSON 内容解析邮箱匹配账号，删除会移除对应 CPA 认证文件。</p>
        <div v-if="boundAccounts.length" class="api-service-account-select-all">
          <a-checkbox
            :model-value="allDeleteSelected"
            :indeterminate="deleteSelectionIndeterminate"
            @change="(checked) => toggleAllDeleteAccounts(Boolean(checked))"
          >
            全选
          </a-checkbox>
          <span>已选 {{ selectedDeleteCount }} / {{ boundAccounts.length }}</span>
        </div>
        <div class="api-service-account-list">
          <label v-for="account in boundAccounts" :key="account.path" class="api-service-account-row">
            <a-checkbox
              :model-value="selectedDeleteEmails.has(account.email)"
              @change="(checked) => toggleDeleteAccount(account.email, Boolean(checked))"
            />
            <div class="api-service-account-main">
              <strong>{{ account.email }}</strong>
              <span>CPA 认证账号</span>
              <template v-if="boundSourceAccount(account)?.quota">
                <div class="api-service-account-quota">
                  <div
                    v-if="boundSourceAccount(account)!.quota!.hourly_window_present !== false"
                    class="api-service-quota-line"
                  >
                    <span>
                      <icon-calendar v-if="isFreePlanAccount(boundSourceAccount(account)!)" />
                      <icon-clock-circle v-else />
                      {{ isFreePlanAccount(boundSourceAccount(account)!) ? "长周期" : "短周期" }}
                    </span>
                    <strong :style="{ color: quotaColor(boundSourceAccount(account)!.quota!.hourly_percentage) }">
                      {{ boundSourceAccount(account)!.quota!.hourly_percentage }}%
                    </strong>
                    <small>{{ quotaWindowLabel(boundSourceAccount(account)!.quota!.hourly_window_minutes, '5 小时窗口') }}</small>
                    <em>{{ quotaResetLeftLabel(boundSourceAccount(account)!.quota!.hourly_reset_time) }}</em>
                  </div>
                  <div
                    v-if="!isFreePlanAccount(boundSourceAccount(account)!) && boundSourceAccount(account)!.quota!.weekly_window_present !== false"
                    class="api-service-quota-line"
                  >
                    <span><icon-calendar /> 长周期</span>
                    <strong :style="{ color: quotaColor(boundSourceAccount(account)!.quota!.weekly_percentage) }">
                      {{ boundSourceAccount(account)!.quota!.weekly_percentage }}%
                    </strong>
                    <small>{{ quotaWindowLabel(boundSourceAccount(account)!.quota!.weekly_window_minutes, '7 天窗口') }}</small>
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
          <a-empty v-if="!boundAccounts.length" description="认证目录里暂无账号" />
        </div>
        <div class="api-service-modal-actions">
          <a-button @click="deleteVisible = false">取消</a-button>
          <a-button status="danger" :loading="deletingBoundAccounts" @click="deleteSelectedBoundAccounts">
            确认删除
          </a-button>
        </div>
      </div>
    </a-modal>
  </section>
</template>
