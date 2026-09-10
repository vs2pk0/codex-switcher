<script setup lang="ts">
import InstanceTransfer from "./InstanceTransfer.vue";
import AppBusyOverlay from "../components/AppBusyOverlay.vue";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { instanceDisplayName } from "../services/instances";
import type { CodexInstance } from "../services/instances";
import {
  getServiceOauthBinding,
  type ServiceOauthBinding,
  type ServiceOauthBindingRequest,
} from "../services/codex";
import { currentLanguage, formatTranslatedText, t } from "../i18n";
import { filterMigrationAccounts, toggleVisibleAccounts } from "./accounts";
import {
  deleteOpenCodexSwitcherAccount,
  deleteOpenCodexEngine,
  getOpenCodexEngineCatalog,
  getOpenCodexImageGenerationSettings,
  getOpenCodexSnapshot,
  getOpenCodexVisionSidecarSettings,
  getOpenCodexVisionModels,
  importOpenCodexSwitcherAccounts,
  installOpenCodexEngine,
  openOpenCodexDashboard,
  readOpenCodexLogs,
  runOpenCodexAction,
  scanOpenCodexSwitcherAccounts,
  subscribeOpenCodexEvents,
  subscribeOpenCodexEngineProgress,
  updateOpenCodexImageGenerationSettings,
  updateOpenCodexVisionSidecarSettings,
  updateOpenCodexVisionModels,
  writeOpenCodexInput,
} from "./service";
import {
  DEFAULT_OPEN_CODEX_PORT,
  isOpenCodexPortPrompt,
  normalizeOpenCodexSettings,
  serializeOpenCodexSettings,
} from "./settings";
import type {
  OpenCodexAction,
  OpenCodexCommandLogEvent,
  OpenCodexEngineCatalog,
  OpenCodexEngineRelease,
  OpenCodexEngineProgress,
  OpenCodexImageGenerationSettings,
  OpenCodexPage,
  OpenCodexSettings,
  OpenCodexSwitcherAccountScan,
  OpenCodexSystemSnapshot,
  OpenCodexVisionBackend,
  OpenCodexVisionModelCatalog,
  OpenCodexVisionSidecarResponse,
} from "./types";

const props = defineProps<{ active: boolean; instances: CodexInstance[] }>();
const emit = defineEmits<{
  (event: "accounts-refreshed"): void;
  (event: "instances-refreshed"): void;
  /** 请求为所选实例绑定 / 更换 / 取消 OAuth 登录态（由 App 复用 OAuth 绑定弹窗处理）。 */
  (event: "bind-oauth", request: ServiceOauthBindingRequest): void;
}>();

const page = ref<OpenCodexPage>("console");
const snapshot = ref<OpenCodexSystemSnapshot | null>(null);
const logs = ref<OpenCodexCommandLogEvent[]>([]);
const busy = ref(false);
/** 同步配置期间的全屏遮罩：同步 → 一键修复 → 重新打开实例，全程禁止操作其他页面。 */
const syncOverlay = ref<{ title: string; message: string; steps: string[]; activeStep: number } | null>(null);
const SYNC_OVERLAY_STEP_MARKERS: Array<{ marker: string; step: number }> = [
  { marker: "已停止实例", step: 0 },
  { marker: "正在绑定 OAuth 账号登录态", step: 1 },
  { marker: "修复切号会话", step: 2 },
  { marker: "恢复全部会话的完整历史", step: 3 },
  { marker: "正在重新打开实例", step: 4 },
];

function syncOverlaySteps(): string[] {
  return [t("重置并同步配置"), t("绑定 OAuth 登录态"), t("修复切号会话"), t("恢复全部会话的完整历史"), t("重新打开 Codex 实例")];
}

/** 根据后端日志行推进遮罩步骤。 */
function advanceSyncOverlay(line: string): void {
  if (!syncOverlay.value) return;
  const matched = SYNC_OVERLAY_STEP_MARKERS.find((item) => line.includes(item.marker));
  if (!matched || matched.step < syncOverlay.value.activeStep) return;
  syncOverlay.value = { ...syncOverlay.value, activeStep: matched.step };
}
const loading = ref(false);
const interactiveOperationId = ref("");
const commandInput = ref("");
const catalog = ref<OpenCodexEngineCatalog | null>(null);
const catalogLoading = ref(false);
const selectedVersion = ref("");
const installingVersion = ref("");
const engineProgress = ref<OpenCodexEngineProgress | null>(null);
const engineError = ref("");
const accountScan = ref<OpenCodexSwitcherAccountScan | null>(null);
const accountScanLoading = ref(false);
const selectedAccountIds = ref<string[]>([]);
const importingAccounts = ref(false);
const deletingAccountId = ref("");
const accountSearch = ref("");
const accountStatus = ref("");
const accountPlan = ref("");
const visionCatalog = ref<OpenCodexVisionModelCatalog | null>(null);
const visionSidecar = ref<OpenCodexVisionSidecarResponse | null>(null);
const visionSidecarDraft = ref<{ enabled: boolean; model: string; backend: OpenCodexVisionBackend }>({ enabled: true, model: "", backend: "openai" });
const visionSidecarError = ref("");
const visionLoading = ref(false);
const visionSaving = ref(false);
const visionSidecarSaving = ref(false);
const visionSearch = ref("");
const selectedVisionModels = ref<string[]>([]);
// Codex「Image Gen」图片生成上游：空字符串表示交给 OpenCodex 默认顺序（ChatGPT 转发 / OpenAI API Key）。
const IMAGE_GEN_DEFAULT_PROVIDER = "";
const IMAGE_GEN_DEFAULT_TIMEOUT_SECONDS = 120;
const imageGen = ref<OpenCodexImageGenerationSettings | null>(null);
const imageGenDraft = ref<{ provider: string; timeoutSeconds: number }>({ provider: IMAGE_GEN_DEFAULT_PROVIDER, timeoutSeconds: IMAGE_GEN_DEFAULT_TIMEOUT_SECONDS });
const imageGenLoading = ref(false);
const imageGenSaving = ref(false);
const imageGenError = ref("");
const selectedInstanceId = ref("default");
const selectedInstance = computed(() => props.instances.find((instance) => instance.id === selectedInstanceId.value));
watch(() => props.instances, (instances) => {
  if (!instances.some((instance) => instance.id === selectedInstanceId.value)) selectedInstanceId.value = "default";
}, { immediate: true });

const transferring = ref(false);
const selectionLocked = computed(() => busy.value || importingAccounts.value || Boolean(deletingAccountId.value)
  || visionSaving.value || visionSidecarSaving.value || imageGenSaving.value || loading.value || catalogLoading.value || accountScanLoading.value || visionLoading.value || transferring.value);
watch(selectedInstanceId, async () => {
  snapshot.value = null; catalog.value = null; accountScan.value = null; visionCatalog.value = null; visionSidecar.value = null;
  selectedAccountIds.value = []; selectedVisionModels.value = []; logs.value = [];
  visionSidecarDraft.value = { enabled: true, model: "", backend: "openai" };
  visionSidecarError.value = "";
  imageGen.value = null; imageGenError.value = "";
  imageGenDraft.value = { provider: IMAGE_GEN_DEFAULT_PROVIDER, timeoutSeconds: IMAGE_GEN_DEFAULT_TIMEOUT_SECONDS };
  selectedVersion.value = ""; engineProgress.value = null; engineError.value = "";
  interactiveOperationId.value = ""; answeredPortPrompts.clear();
  settings.value = loadSettings();
  const id = selectedInstanceId.value;
  await refreshSnapshot();
  const persisted = await readOpenCodexLogs(300,id).catch(() => []);
  if (id !== selectedInstanceId.value) return;
  logs.value = persisted.map(line => ({ operationId: "history", stream: "system", line, timestamp: new Date().toISOString() }));
  if (page.value === "versions") await checkVersions();
  if (page.value === "vision") await loadVisionModels();
});
let unlistenEvents: UnlistenFn | undefined;
let unlistenEngine: UnlistenFn | undefined;
let disposed = false;
const answeredPortPrompts = new Set<string>();

function loadSettings(): OpenCodexSettings {
  try {
    const stored = localStorage.getItem(`codex-switcher-opencodex-settings-${selectedInstanceId.value}`);
    return normalizeOpenCodexSettings(stored ? JSON.parse(stored) : undefined);
  } catch {
    return normalizeOpenCodexSettings(undefined);
  }
}

const settings = ref<OpenCodexSettings>(loadSettings());
const effectivePort = computed(() => snapshot.value?.port || settings.value.port);
const recentLogs = computed(() => logs.value.slice(-14));
const selectedRelease = computed(
  () => catalog.value?.releases.find((release) => release.version === selectedVersion.value) ?? null,
);
const accountMutationBusy = computed(() => busy.value || importingAccounts.value || Boolean(deletingAccountId.value));
const filteredAccounts = computed(() => filterMigrationAccounts(
  accountScan.value?.accounts ?? [], accountSearch.value, accountStatus.value, accountPlan.value,
));
const accountPlans = computed(() => [...new Set((accountScan.value?.accounts ?? []).map((account) => account.plan || "__unknown__"))].sort());
const visibleSelectable = computed(() => filteredAccounts.value.filter((account) => account.eligible || account.deletable));
const visibleSelectedCount = computed(() => visibleSelectable.value.filter((account) => selectedAccountIds.value.includes(account.sourceId)).length);
const allVisibleSelected = computed(() => visibleSelectable.value.length > 0 && visibleSelectedCount.value === visibleSelectable.value.length);
const hiddenSelectedCount = computed(() => selectedAccountIds.value.filter((id) => !filteredAccounts.value.some((account) => account.sourceId === id)).length);
const engineStageLabel = computed(() => t(({
  checking: "检查版本", downloading: "下载 Engine 包", dependencies: "安装依赖",
  validating: "校验 Engine", stopping: "停止当前服务", activating: "切换 Engine",
  restarting: "重启并检查服务", recovering: "恢复原 Engine", complete: "Engine 更新完成", error: "Engine 操作失败",
})[engineProgress.value?.stage ?? "checking"]));
const engineDownloadPercent = computed(() => {
  const progress = engineProgress.value;
  return progress?.stage === "downloading" && progress.totalBytes && progress.totalBytes > 0
    ? Math.min(1, Math.max(0, (progress.downloadedBytes ?? 0) / progress.totalBytes)) : null;
});
function formatBytes(bytes: number): string {
  return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
function selectVisibleAccounts(checked: boolean | (string | number | boolean)[]): void {
  if (accountMutationBusy.value || accountScanLoading.value) return;
  selectedAccountIds.value = toggleVisibleAccounts(selectedAccountIds.value, filteredAccounts.value, checked === true);
}
const selectedImportAccountIds = computed(() => selectedAccountIds.value.filter((sourceId) =>
  accountScan.value?.accounts.some((account) => account.sourceId === sourceId && account.eligible),
));
const selectedDeleteAccounts = computed(() => (accountScan.value?.accounts ?? []).filter((account) =>
  selectedAccountIds.value.includes(account.sourceId) && account.deletable,
));
const filteredVisionModels = computed(() => {
  const query = visionSearch.value.trim().toLowerCase();
  return (visionCatalog.value?.models ?? []).filter((model) =>
    !query || model.namespaced.toLowerCase().includes(query),
  );
});
const sidecarSelectableModels = computed(() =>
  filteredVisionModels.value.filter((model) => !model.nativeVision && !model.disabled),
);
const selectedVisionCount = computed(() => selectedVisionModels.value.length);
const visionSidecarModelOptions = computed(() => visionSidecar.value?.visionModels ?? []);
// label 为中文源文案，渲染时经 t() 翻译
const visionBackendOptions: Array<{ value: OpenCodexVisionBackend; label: string }> = [
  { value: "openai", label: "OpenAI 接口直连" },
  { value: "anthropic", label: "Anthropic 接口直连" },
  { value: "routed", label: "按 Provider 路由转发" },
];

function applyVisionSidecarResponse(response: OpenCodexVisionSidecarResponse): void {
  visionSidecar.value = response;
  const option = response.visionModels.find((item) => item.value === response.vision.model);
  visionSidecarDraft.value = {
    enabled: response.vision.enabled,
    model: response.vision.model,
    backend: response.vision.backend ?? option?.backend ?? (response.vision.model.includes("/") ? "routed" : "openai"),
  };
  if (visionCatalog.value) {
    visionCatalog.value = {
      ...visionCatalog.value,
      sidecarModel: response.vision.model,
      sidecarBackend: visionSidecarDraft.value.backend,
    };
  }
}

const imageGenOptions = computed(() => imageGen.value?.options ?? []);
const selectedImageGenOption = computed(() =>
  imageGenOptions.value.find((option) => option.name === imageGenDraft.value.provider),
);
/** 当前选择对应的说明：默认路径是否可用、是否会创建镜像提供方。 */
const imageGenHint = computed(() => {
  const option = selectedImageGenOption.value;
  if (!option) {
    return imageGen.value?.openaiUpstreamAvailable
      ? t("默认由 OpenCodex 内置的 ChatGPT 转发或 OpenAI API Key 上游处理")
      : t("当前没有 ChatGPT 转发或 OpenAI API Key 上游，Codex 生图会直接报错，请选择一个提供方");
  }
  return option.direct
    ? formatTranslatedText("生图请求会直接转发到 {name} 的 /v1/images 接口", { name: option.name })
    : formatTranslatedText("保存时会创建镜像提供方 {mirror} 承接 /v1/images，不会增加聊天模型", { mirror: `${option.name}-images` });
});
const imageGenHintWarning = computed(() => !selectedImageGenOption.value && !imageGen.value?.openaiUpstreamAvailable);
const imageGenRecentRequests = computed(() => imageGen.value?.recentRequests ?? []);
const imageGenLatestFailed = computed(() => (imageGenRecentRequests.value[0]?.status ?? 0) >= 400);

function formatImageGenTime(timestamp: number): string {
  return new Date(timestamp).toLocaleString(undefined, { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

function applyImageGenerationSettings(settings: OpenCodexImageGenerationSettings): void {
  imageGen.value = settings;
  imageGenDraft.value = {
    provider: settings.provider ?? IMAGE_GEN_DEFAULT_PROVIDER,
    timeoutSeconds: settings.timeoutMs ? Math.round(settings.timeoutMs / 1000) : IMAGE_GEN_DEFAULT_TIMEOUT_SECONDS,
  };
}

async function loadImageGenerationSettings(): Promise<void> {
  imageGenLoading.value = true;
  imageGenError.value = "";
  try {
    const instanceId = selectedInstanceId.value;
    const settings = await getOpenCodexImageGenerationSettings(instanceId);
    if (instanceId !== selectedInstanceId.value) return;
    applyImageGenerationSettings(settings);
  } catch (error) {
    imageGen.value = null;
    imageGenError.value = errorText(error);
  } finally {
    imageGenLoading.value = false;
  }
}

async function saveImageGenerationSettings(): Promise<void> {
  const provider = imageGenDraft.value.provider || null;
  const timeoutSeconds = Math.round(Number(imageGenDraft.value.timeoutSeconds));
  if (!Number.isFinite(timeoutSeconds) || timeoutSeconds < 5 || timeoutSeconds > 300) {
    Message.warning(t("图片生成超时需在 5 到 300 秒之间"));
    return;
  }
  imageGenSaving.value = true;
  imageGenError.value = "";
  try {
    const result = await updateOpenCodexImageGenerationSettings(
      { provider, timeoutMs: timeoutSeconds * 1000 },
      selectedInstanceId.value,
    );
    applyImageGenerationSettings(result.settings);
    Message.success(result.restarted ? t("图片生成设置已保存，OpenCodex 已重启") : t("图片生成设置已保存，将在下次启动时生效"));
  } catch (error) {
    imageGenError.value = errorText(error);
    Message.error(formatTranslatedText("保存图片生成设置失败：{error}", { error: imageGenError.value }));
  } finally {
    imageGenSaving.value = false;
  }
}

function selectVisionSidecarModel(value: unknown): void {
  if (typeof value !== "string") return;
  const option = visionSidecarModelOptions.value.find((item) => item.value === value);
  visionSidecarDraft.value.model = value;
  if (option) visionSidecarDraft.value.backend = option.backend;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function appendLog(event: OpenCodexCommandLogEvent): void {
  if (event.operationId !== "history" && !event.operationId.startsWith(`${selectedInstanceId.value}:`)) return;
  logs.value = [...logs.value.slice(-999), event];
  if (event.operationId !== "history" && event.stream === "system") advanceSyncOverlay(event.line);
  if (isOpenCodexPortPrompt(event.line) && !answeredPortPrompts.has(event.operationId)) {
    answeredPortPrompts.add(event.operationId);
    const selectedPort = settings.value.port || DEFAULT_OPEN_CODEX_PORT;
    void writeOpenCodexInput(event.operationId, `${selectedPort}\n`, selectedInstanceId.value)
      .then(() => {
        logs.value = [...logs.value.slice(-999), {
          operationId: event.operationId,
          stream: "system",
          line: formatTranslatedText("已使用管理器默认端口 {port}", { port: selectedPort }),
          timestamp: new Date().toISOString(),
        }];
      })
      .catch((error) => Message.error(formatTranslatedText("自动填写 OpenCodex 端口失败：{error}", { error: errorText(error) })));
  }
}

async function refreshSnapshot(showError = true, quiet = false): Promise<void> {
  if (!quiet) loading.value = true;
  try {
    const instanceId = selectedInstanceId.value;
    const result = await getOpenCodexSnapshot(instanceId);
    if (instanceId !== selectedInstanceId.value) return;
    snapshot.value = result;
    if (!quiet) emit("instances-refreshed");
    if (snapshot.value.port) settings.value.port = snapshot.value.port;
    void refreshOauthBinding();
  } catch (error) {
    if (showError) Message.error(formatTranslatedText("读取 OpenCodex 状态失败：{error}", { error: errorText(error) }));
  } finally {
    if (!quiet) loading.value = false;
  }
}

/** 所选实例当前绑定的 OAuth 登录态（auth.json 推断），用于快捷操作卡片展示。 */
const oauthBinding = ref<ServiceOauthBinding | null>(null);
async function refreshOauthBinding(): Promise<void> {
  const instanceId = selectedInstanceId.value;
  try {
    const binding = await getServiceOauthBinding("opencodex", instanceId);
    if (instanceId !== selectedInstanceId.value) return;
    oauthBinding.value = binding;
  } catch {
    if (instanceId === selectedInstanceId.value) oauthBinding.value = null;
  }
}

function requestOauthBinding(): void {
  const instanceId = selectedInstanceId.value;
  emit("bind-oauth", {
    kind: "opencodex",
    instanceId,
    instanceName: selectedInstance.value ? instanceDisplayName(selectedInstance.value) : instanceId,
    onUpdated: (binding) => {
      if (binding.instanceId === selectedInstanceId.value) oauthBinding.value = binding;
      void refreshSnapshot(false, true);
    },
  });
}

async function executeAction(action: OpenCodexAction, instanceId = selectedInstanceId.value): Promise<void> {
  if (busy.value) return;
  if (
    action === "restore"
    || action === "uninstall"
    || action === "service_install"
    || action === "service_uninstall"
  ) {
    const confirmed = await new Promise<boolean>((resolve) => {
      const confirmation = action === "restore"
        ? {
            title: t("恢复原生 Codex"),
            content: `${selectedInstance.value ? instanceDisplayName(selectedInstance.value) : t("系统默认实例（原版）")}：${t("仅恢复所选实例的原生配置，其他实例不受影响。是否继续？")}`,
          }
        : action === "uninstall"
          ? {
              title: t("卸载 OpenCodex"),
              content: t("将恢复所选实例的原生 Codex 配置，并卸载此实例的 OpenCodex。是否继续？"),
            }
          : action === "service_install"
            ? {
                title: t("开启后台服务"),
                content: t("将注册并启动 OpenCodex 系统后台服务，使其在登录后自动运行。是否继续？"),
              }
            : {
                title: t("取消后台服务"),
                content: t("将恢复所选实例，再停止并移除该实例后台服务；账号和配置仍会保留。是否继续？"),
              };
      Modal.warning({
        title: confirmation.title,
        content: confirmation.content,
        okText: t("确认执行"),
        cancelText: t("取消"),
        hideCancel: false,
        onOk: () => resolve(true),
        onCancel: () => resolve(false),
        onClose: () => resolve(false),
      });
    });
    if (!confirmed) return;
  }
  busy.value = true;
  if (action === "sync") {
    syncOverlay.value = {
      title: `${t("正在同步配置并一键修复")}「${selectedInstance.value ? instanceDisplayName(selectedInstance.value) : t("系统默认实例（原版）")}」`,
      message: t("同步配置后会先对该实例执行一键修复（切号修复 + 恢复全部完整会话），修复完成后再重新打开 Codex。请勿关闭应用。"),
      steps: syncOverlaySteps(),
      activeStep: 0,
    };
  }
  try {
    const started = await runOpenCodexAction(action, settings.value.port, instanceId);
    interactiveOperationId.value = started.interactive ? started.operationId : "";
    if (started.interactive) page.value = "console";
  } catch (error) {
    busy.value = false;
    syncOverlay.value = null;
    Message.error(formatTranslatedText("OpenCodex 操作启动失败：{error}", { error: errorText(error) }));
  }
}

async function run(action: OpenCodexAction): Promise<void> {
  if (busy.value) return;
  if (!snapshot.value?.installed) {
    Message.warning(t("尚未安装或选择 OpenCodex Engine，请先在版本管理中下载并激活版本"));
    page.value = "versions";
    return;
  }
  if (action === "sync" || action === "restore") {
    await executeAction(action, selectedInstanceId.value);
    return;
  }
  await executeAction(action);
}

async function submitCommandInput(): Promise<void> {
  const operationId = interactiveOperationId.value;
  if (!operationId) return;
  try {
    await writeOpenCodexInput(operationId, `${commandInput.value}\n`, selectedInstanceId.value);
    commandInput.value = "";
  } catch (error) {
    Message.error(formatTranslatedText("发送初始化输入失败：{error}", { error: errorText(error) }));
  }
}

async function openDashboard(mode = settings.value.dashboardOpenMode): Promise<void> {
  try {
    await openOpenCodexDashboard(mode, effectivePort.value, selectedInstanceId.value);
  } catch (error) {
    Message.error(formatTranslatedText("打开 OpenCodex Dashboard 失败：{error}", { error: errorText(error) }));
  }
}

async function checkVersions(): Promise<void> {
  if (catalogLoading.value) return;
  catalogLoading.value = true;
  try {
    const instanceId = selectedInstanceId.value;
    const result = await getOpenCodexEngineCatalog(instanceId);
    if (instanceId !== selectedInstanceId.value) return;
    catalog.value = result;
    selectedVersion.value =
      selectedVersion.value ||
      catalog.value.latestStable?.version ||
      catalog.value.releases[0]?.version ||
      "";
    if (catalog.value.remoteError) {
      Message.warning(formatTranslatedText("GitHub 版本暂时不可用，本地 Engine 仍可管理：{error}", { error: catalog.value.remoteError }));
    }
  } catch (error) {
    Message.error(formatTranslatedText("检测 OpenCodex Engine 版本失败：{error}", { error: errorText(error) }));
  } finally {
    catalogLoading.value = false;
  }
}

async function applyRelease(release: OpenCodexEngineRelease): Promise<void> {
  await maintainEngine(release.version);
}

async function switchInstalledVersion(version: string): Promise<void> {
  await maintainEngine(version);
}

async function maintainEngine(version: string): Promise<void> {
  if (accountMutationBusy.value || !version) return;
  const operationId = crypto.randomUUID();
  busy.value = true;
  installingVersion.value = version;
  engineError.value = "";
  engineProgress.value = { operationId, version, stage: "checking" };
  try {
    unlistenEngine = await subscribeOpenCodexEngineProgress(operationId, (progress) => { engineProgress.value = progress; });
    if (disposed) return;
    const result = await installOpenCodexEngine(version, operationId, selectedInstanceId.value);
    engineProgress.value = { operationId, version: result.version, stage: "complete" };
    Message.success(result.message);
  } catch (error) {
    engineProgress.value = { operationId, version, stage: "error" };
    engineError.value = errorText(error);
    Message.error(formatTranslatedText("安装 Engine 失败：{error}", { error: engineError.value }));
  } finally {
    unlistenEngine?.();
    unlistenEngine = undefined;
    if (!disposed) await Promise.all([refreshSnapshot(false, true), checkVersions()]);
    installingVersion.value = "";
    busy.value = false;
  }
}

function confirmDeleteInstalledVersion(version: string): void {
  if (accountMutationBusy.value || (catalog.value?.currentVersion === version && snapshot.value?.running)) return;
  const instanceId = selectedInstanceId.value;
  const fullRemoval = catalog.value?.installedVersions.length === 1;
  Modal.warning({
    title: t("删除本地 Engine"),
    content: fullRemoval
      ? `目标：${selectedInstance.value ? instanceDisplayName(selectedInstance.value) : instanceId}（${snapshot.value?.dataDir || ''}）。这是最后一个 Engine。确认删除 v${version} 及该实例的全部 OpenCodex 配置、账号、历史、备份和数据目录？将先解除 Codex 接入，不删除 Codex 会话或其他实例。此操作不可撤销。`
      : formatTranslatedText("确认删除本地 Engine v{version}？若为当前版本，将切换到剩余版本并保持停止。", { version }),
    okText: t("删除"),
    cancelText: t("取消"),
    hideCancel: false,
    async onOk() {
      if (accountMutationBusy.value) return false;
      busy.value = true;
      try {
        const result = await deleteOpenCodexEngine(version, instanceId, fullRemoval);
        if (fullRemoval) localStorage.removeItem(`codex-switcher-opencodex-settings-${instanceId}`);
        Message.success(result.message);
        engineProgress.value = null;
        await Promise.all([checkVersions(), refreshSnapshot(false, true)]);
        emit("instances-refreshed");
      } catch (error) {
        Message.error(formatTranslatedText("删除 Engine 失败：{error}", { error: errorText(error) }));
      } finally {
        busy.value = false;
      }
    },
  });
}

function saveSettings(): void {
  if (!Number.isInteger(settings.value.port) || settings.value.port < 1024 || settings.value.port > 65535) {
    Message.warning(t("端口必须在 1024–65535 之间"));
    return;
  }
  localStorage.setItem(
    `codex-switcher-opencodex-settings-${selectedInstanceId.value}`,
    serializeOpenCodexSettings(settings.value),
  );
  Message.success(t("OpenCodex 设置已保存，下一次启动服务时生效"));
}

async function scanAccounts(): Promise<void> {
  if (accountScanLoading.value) return;
  accountScanLoading.value = true;
  try {
    const initial = !accountScan.value;
    const instanceId = selectedInstanceId.value;
    const result = await scanOpenCodexSwitcherAccounts(instanceId);
    if (instanceId !== selectedInstanceId.value) return;
    accountScan.value = result;
    selectedAccountIds.value = accountScan.value.accounts
      .filter((account) => initial ? account.eligible : (account.eligible || account.deletable) && selectedAccountIds.value.includes(account.sourceId))
      .map((account) => account.sourceId);
  } catch (error) {
    Message.error(formatTranslatedText("扫描 Switcher 账号失败：{error}", { error: errorText(error) }));
  } finally {
    accountScanLoading.value = false;
  }
}

async function loadVisionModels(): Promise<void> {
  if (!snapshot.value?.running) {
    visionCatalog.value = null;
    visionSidecar.value = null;
    selectedVisionModels.value = [];
    return;
  }
  visionLoading.value = true;
  visionSidecarError.value = "";
  // 图片生成上游只读 config.json，与模型目录并行读取即可。
  void loadImageGenerationSettings();
  try {
    const instanceId = selectedInstanceId.value;
    const result = await getOpenCodexVisionModels(instanceId);
    if (instanceId !== selectedInstanceId.value) return;
    visionCatalog.value = result;
    selectedVisionModels.value = visionCatalog.value.models
      .filter((model) => model.sidecarEnabled)
      .map((model) => model.namespaced);
    try {
      const sidecar = await getOpenCodexVisionSidecarSettings(instanceId);
      if (instanceId !== selectedInstanceId.value) return;
      applyVisionSidecarResponse(sidecar);
    } catch (error) {
      visionSidecar.value = null;
      visionSidecarError.value = errorText(error);
    }
  } catch (error) {
    Message.error(formatTranslatedText("读取 OpenCodex 模型失败：{error}", { error: errorText(error) }));
  } finally {
    visionLoading.value = false;
  }
}

async function saveVisionSidecarSettings(): Promise<void> {
  const model = visionSidecarDraft.value.model.trim();
  if (!model) {
    Message.warning(t("请选择图片描述模型"));
    return;
  }
  visionSidecarSaving.value = true;
  visionSidecarError.value = "";
  try {
    const result = await updateOpenCodexVisionSidecarSettings({
      enabled: visionSidecarDraft.value.enabled,
      model,
      backend: visionSidecarDraft.value.backend ?? null,
    }, selectedInstanceId.value);
    applyVisionSidecarResponse(result);
    Message.success(t("图片描述模型已保存"));
  } catch (error) {
    visionSidecarError.value = errorText(error);
    Message.error(formatTranslatedText("保存图片模型失败：{error}", { error: visionSidecarError.value }));
  } finally {
    visionSidecarSaving.value = false;
  }
}

function toggleVisionModel(namespaced: string): void {
  selectedVisionModels.value = selectedVisionModels.value.includes(namespaced)
    ? selectedVisionModels.value.filter((model) => model !== namespaced)
    : [...selectedVisionModels.value, namespaced];
}

function selectFilteredVisionModels(): void {
  const visible = sidecarSelectableModels.value.map((model) => model.namespaced);
  selectedVisionModels.value = [...new Set([...selectedVisionModels.value, ...visible])];
}

function clearVisionModels(): void {
  selectedVisionModels.value = [];
}

async function saveVisionModels(): Promise<void> {
  const models = (visionCatalog.value?.models ?? [])
    .filter((model) => selectedVisionModels.value.includes(model.namespaced))
    .map((model) => ({ provider: model.provider, id: model.id }));
  visionSaving.value = true;
  try {
    const result = await updateOpenCodexVisionModels(models, selectedInstanceId.value);
    emit("instances-refreshed");
    Message.success(result.message);
    await Promise.all([refreshSnapshot(false), loadVisionModels()]);
    if (result.changedProviders.length) {
      Modal.info({
        title: t("图片模型已同步"),
        content: t("Codex 客户端会缓存模型能力。请用 Command + Q（Windows 使用退出菜单）完全退出 Codex，再重新打开后使用图片输入。"),
        okText: t("知道了"),
      });
    }
  } catch (error) {
    Message.error(formatTranslatedText("保存图片模型失败：{error}", { error: errorText(error) }));
  } finally {
    visionSaving.value = false;
  }
}

async function importAccounts(): Promise<void> {
  if (accountMutationBusy.value || accountScanLoading.value || snapshot.value?.running) return;
  if (!selectedImportAccountIds.value.length) {
    Message.warning(t("请至少选择一个可导入账号"));
    return;
  }
  importingAccounts.value = true;
  try {
    const result = await importOpenCodexSwitcherAccounts(selectedImportAccountIds.value, selectedInstanceId.value);
    Message.success(formatTranslatedText("已导入 {importedCount} 个账号，跳过 {skippedCount} 个", {
      importedCount: result.importedCount,
      skippedCount: result.skippedCount,
    }));
    emit("accounts-refreshed");
    await scanAccounts();
  } catch (error) {
    Message.error(formatTranslatedText("导入 OpenCodex 账号失败：{error}", { error: errorText(error) }));
  } finally {
    importingAccounts.value = false;
  }
}

function confirmDeleteSelectedAccounts(): void {
  if (accountMutationBusy.value || accountScanLoading.value) return;
  const selected = [...selectedDeleteAccounts.value];
  if (!selected.length || deletingAccountId.value) {
    Message.warning(t("请至少选择一个已导入且可删除的账号"));
    return;
  }
  if (snapshot.value?.running) {
    Message.warning(t("请先停止 OpenCodex 服务，再删除账号"));
    return;
  }
  Modal.warning({
    title: t("批量删除 OpenCodex 账号"),
    content: formatTranslatedText("确认从 OpenCodex 删除所选 {count} 个账号？Switcher 账号总览中的原账号会保留。", { count: selected.length }),
    okText: t("删除"),
    cancelText: t("取消"),
    hideCancel: false,
    onOk: async () => {
      if (accountMutationBusy.value || snapshot.value?.running) return false;
      deletingAccountId.value = "__selected__";
      try {
        const results = [];
        for (const account of selected) {
          results.push(await deleteOpenCodexSwitcherAccount(account.sourceId, selectedInstanceId.value));
        }
        const deletedCount = results.filter((result) => result.deleted).length;
        const failures = results.filter((result) => !result.deleted && /身份|不匹配|无法确认/.test(result.message));
        if (failures.length) {
          Message.warning(formatTranslatedText("已删除 {deletedCount} 个账号，{failureCount} 个账号因身份校验失败而保留", { deletedCount, failureCount: failures.length }));
        } else {
          Message.success(formatTranslatedText("已从 OpenCodex 删除 {count} 个账号", { count: deletedCount }));
        }
        emit("accounts-refreshed");
        await scanAccounts();
      } catch (error) {
        Message.error(formatTranslatedText("批量删除 OpenCodex 账号失败：{error}", { error: errorText(error) }));
        throw error;
      } finally {
        deletingAccountId.value = "";
      }
    },
  });
}

function confirmDeleteMigratedAccount(
  account: OpenCodexSwitcherAccountScan["accounts"][number],
): void {
  if (accountMutationBusy.value || accountScanLoading.value) return;
  if (!account.deletable || deletingAccountId.value) return;
  if (snapshot.value?.running) {
    Message.warning(t("请先停止 OpenCodex 服务，再删除账号"));
    return;
  }
  Modal.warning({
    title: t("删除 OpenCodex 账号"),
    content: formatTranslatedText("确认从 OpenCodex 删除 {account}？Switcher 账号总览中的原账号会保留。", { account: account.email || account.sourceId }),
    okText: t("删除"),
    cancelText: t("取消"),
    hideCancel: false,
    onOk: async () => {
      if (accountMutationBusy.value || snapshot.value?.running) return false;
      deletingAccountId.value = account.sourceId;
      try {
        const result = await deleteOpenCodexSwitcherAccount(account.sourceId, selectedInstanceId.value);
        Message.success(result.message);
        emit("accounts-refreshed");
        await scanAccounts();
      } catch (error) {
        Message.error(formatTranslatedText("删除 OpenCodex 账号失败：{error}", { error: errorText(error) }));
        throw error;
      } finally {
        deletingAccountId.value = "";
      }
    },
  });
}

function formatLogTime(value: string): string {
  const date = new Date(value);
  const locale = currentLanguage.value === "ru" ? "ru-RU" : currentLanguage.value === "en" ? "en-US" : currentLanguage.value === "zh-TW" ? "zh-TW" : "zh-CN";
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleTimeString(locale, { hour12: false });
}

function accountStatusLabel(status: string): string {
  if (status === "ready") return t("待导入");
  if (status === "already_imported") return t("已导入");
  if (status === "unsupported") return t("不支持");
  return t("无效");
}

function backgroundServiceSummary(): string {
  const service = snapshot.value?.backgroundService;
  if (!service) return "";
  if (!service.supported) return t("当前系统不支持后台服务");
  if (!service.installed) return t("后台服务尚未安装");
  if (service.conflict) return t("检测到后台服务冲突，请卸载后重新安装");
  if (service.stale) return t("后台服务文件已过期，请重新安装");
  if (service.running) return t("后台服务已安装并正在运行");
  if (service.enabled) return t("后台服务已启用，当前未运行");
  return t("后台服务已安装，当前未启用");
}

function toggleMigrationAccount(account: OpenCodexSwitcherAccountScan["accounts"][number]): void {
  if (accountMutationBusy.value || accountScanLoading.value) return;
  if (!account.eligible && !account.deletable) return;
  selectedAccountIds.value = selectedAccountIds.value.includes(account.sourceId)
    ? selectedAccountIds.value.filter((id) => id !== account.sourceId)
    : [...selectedAccountIds.value, account.sourceId];
}

watch(
  () => props.active,
  (active) => {
    if (active) {
      void refreshSnapshot(false, Boolean(installingVersion.value));
      return;
    }
  },
);

watch(page, (nextPage) => {
  if (nextPage === "versions" && !catalog.value) void checkVersions();
  if (nextPage === "vision") void loadVisionModels();
});

onMounted(async () => {
  try {
    const persisted = await readOpenCodexLogs(300, selectedInstanceId.value);
    logs.value = persisted.map((line, index) => ({
      operationId: "history",
      stream: line.includes("stderr") ? "stderr" : "system",
      line,
      timestamp: new Date(Date.now() - (persisted.length - index) * 10).toISOString(),
    }));
    unlistenEvents = await subscribeOpenCodexEvents(
      appendLog,
      (event) => {
        if (!event.operationId.startsWith(`${selectedInstanceId.value}:`)) return;
        if (!installingVersion.value) busy.value = false;
        if (event.action === "sync") syncOverlay.value = null;
        if (interactiveOperationId.value === event.operationId) interactiveOperationId.value = "";
        appendLog({
          operationId: event.operationId,
          stream: event.success ? "system" : "stderr",
          line: event.message,
          timestamp: event.timestamp,
        });
        if (event.success) Message.success(event.message);
        else Message.error(event.message);
        if (["sync", "restore", "uninstall", "service_uninstall"].includes(event.action)) {
          emit("instances-refreshed");
        }
        void refreshSnapshot(false, true);
      },
    );
    if (disposed) { unlistenEvents(); return; }
    await refreshSnapshot(false);
  } catch (error) {
    Message.error(formatTranslatedText("初始化 OpenCodex 管理模块失败：{error}", { error: errorText(error) }));
  }
});

onUnmounted(() => { disposed = true; unlistenEvents?.(); unlistenEngine?.(); });
</script>

<template>
  <section class="opencodex-page">
    <AppBusyOverlay
      :visible="syncOverlay !== null"
      :title="syncOverlay?.title ?? ''"
      :message="syncOverlay?.message"
      :steps="syncOverlay?.steps"
      :active-step="syncOverlay?.activeStep"
    />
    <header class="opencodex-hero">
      <div>
        <div class="opencodex-title-row">
          <span v-if="page !== 'versions' && page !== 'transfer'" class="opencodex-brand-mark">OC</span>
          <div>
            <h1>{{ page === 'versions' ? t('版本管理') : page === 'transfer' ? t('数据传输') : 'OpenCodex Manager' }}</h1>
            <p>{{ t(page === 'versions' ? '管理当前实例的 Engine，支持安装、切换与回退。' : page === 'transfer' ? '在不同实例之间传输数据，支持复制合并与覆盖。' : '本地代理、Codex 集成与 Engine 版本控制台') }}</p>
          </div>
        </div>
      </div>
      <div class="instance-target-bar">
        <label for="opencodex-instance-target">{{ t("当前实例") }}</label>
        <a-select id="opencodex-instance-target" v-model="selectedInstanceId" :disabled="selectionLocked" :style="{ width: '220px' }">
          <a-option v-if="!instances.some(i => i.id === 'default')" value="default">{{ t("系统默认实例（原版）") }}</a-option>
          <a-option v-for="instance in instances" :key="instance.id" :value="instance.id">{{ instanceDisplayName(instance) }}</a-option>
        </a-select>
      </div>
    </header>
    <div class="instance-meta-row">
      <div class="opencodex-status-strip">
        <span :class="['status-dot', snapshot?.running ? 'online' : 'offline']" />
        <strong>{{ t(snapshot?.running ? "服务运行中" : "服务未运行") }}</strong>
        <span>Engine {{ snapshot?.engineVersion ? `v${snapshot.engineVersion}` : t("不可用") }}</span>
        <span>{{ snapshot?.platform || t("检测中") }}</span>
        <span>{{ t("端口") }} {{ effectivePort }}</span>
      </div>
    <div class="instance-storage-hint"><span>{{ t("独立数据目录") }}</span><code :title="snapshot?.dataDir">{{ snapshot?.dataDir || t("正在读取实例目录…") }}</code></div>
    </div>
    <nav class="opencodex-tabs" :aria-label="t('OpenCodex 功能导航')">
      <button :class="{ active: page === 'console' }" @click="page = 'console'">
        <icon-command />{{ t("运行控制台") }}
      </button>
      <button :class="{ active: page === 'web' }" @click="page = 'web'">
        <icon-public />{{ t("Web 管理") }}
      </button>
      <button :class="{ active: page === 'vision' }" @click="page = 'vision'">
        <icon-image />{{ t("图片模型") }}
      </button>
      <button :class="{ active: page === 'versions' }" @click="page = 'versions'">
        <icon-apps />{{ t("版本管理") }}
      </button>
      <button :class="{ active: page === 'transfer' }" @click="page = 'transfer'"><icon-swap />{{ t("数据传输") }}</button>
      <button :class="{ active: page === 'logs' }" @click="page = 'logs'">
        <icon-file />{{ t("运行日志") }}
      </button>
      <button :class="{ active: page === 'settings' }" @click="page = 'settings'">
        <icon-settings />{{ t("设置") }}
      </button>
    </nav>

    <div v-if="snapshot && !snapshot.installed" class="engine-setup-notice" role="status">
      <icon-info-circle /><div><strong>{{ t("当前实例尚未安装 Engine") }}</strong><span>{{ t("下载并激活版本后，即可初始化此实例的独立服务。") }}</span></div>
      <a-button @click="page = 'versions'">{{ t("选择版本") }}<template #icon><icon-download /></template></a-button>
    </div>

    <section v-if="engineProgress && engineProgress.stage !== 'complete'" class="engine-progress" :class="engineProgress.stage" aria-live="polite">
      <div class="engine-progress-heading"><strong>{{ engineStageLabel }}</strong><span>Engine v{{ engineProgress.version }}</span><span v-if="engineProgress.stage === 'downloading' && engineProgress.downloadedBytes != null">{{ formatBytes(engineProgress.downloadedBytes) }}<template v-if="engineProgress.totalBytes"> / {{ formatBytes(engineProgress.totalBytes) }}</template></span></div>
      <a-progress v-if="engineDownloadPercent !== null" :percent="engineDownloadPercent" animation />
      <div v-else-if="engineProgress.stage !== 'error'" class="engine-indeterminate" role="progressbar" :aria-label="engineStageLabel"><a-progress :percent="0.3" :show-text="false" aria-hidden="true" /></div>
      <p v-if="engineError" class="engine-error">{{ engineError }}</p>
    </section>

    <a-spin :loading="loading" class="opencodex-content" :tip="t('正在读取 OpenCodex 状态…')">
        <section v-if="page !== 'versions' && page !== 'transfer'" class="service-control-card">
          <div class="service-actions">
            <a-button type="primary" :disabled="busy || snapshot?.running || !snapshot?.initialized" @click="run('start')">
              <template #icon><icon-play-arrow /></template>{{ t("启动服务") }}
            </a-button>
            <a-button :disabled="busy || !snapshot?.running" @click="run('stop')">
              <template #icon><icon-pause /></template>{{ t("停止") }}
            </a-button>
            <a-button :disabled="busy || !snapshot?.initialized" @click="run('restart')">
              <template #icon><icon-refresh /></template>{{ t("重启") }}
            </a-button>
            <a-button :disabled="busy" @click="run('init')">
              <template #icon><icon-command /></template>{{ t(snapshot?.initialized ? "重新初始化" : "初始化") }}
            </a-button>
            <a-button type="text" :disabled="loading" @click="refreshSnapshot()">
              <template #icon><icon-sync /></template>{{ t("刷新状态") }}
            </a-button>
          </div>
          <div class="health-indicator" :class="{ healthy: snapshot?.ready }">
            <icon-heart-fill />
            <span><small>{{ t("服务健康") }}</small><strong>{{ t(snapshot?.ready ? "正常" : snapshot?.running ? "正在就绪" : "未运行") }}</strong></span>
          </div>
        </section>
      <template v-if="page === 'console'">
        <section class="overview-grid">
          <article class="overview-card">
            <span class="overview-icon green"><icon-apps /></span>
            <small>{{ t("Engine 版本") }}</small>
            <strong>{{ snapshot?.engineVersion ? `v${snapshot.engineVersion}` : t("资源不可用") }}</strong>
            <p>{{ t(snapshot?.installed ? "在线安装版本" : "请先下载 Engine") }}</p>
          </article>
          <article class="overview-card">
            <span class="overview-icon blue"><icon-thunderbolt /></span>
            <small>{{ t("服务状态") }}</small>
            <strong>{{ t(snapshot?.running ? "运行中" : "已停止") }}</strong>
            <p>{{ snapshot?.pid ? `PID ${snapshot.pid}` : t("暂无运行进程") }}</p>
          </article>
          <article class="overview-card access-card">
            <span class="overview-icon cyan"><icon-public /></span>
            <small>{{ t("访问地址") }}</small>
            <strong>localhost:{{ effectivePort }}</strong>
            <p>
              <a-link :disabled="!snapshot?.running" @click="openDashboard('client')">{{ t("客户端打开") }}</a-link>
              <a-link :disabled="!snapshot?.running" @click="openDashboard('browser')">{{ t("浏览器打开") }}</a-link>
            </p>
          </article>
          <article class="overview-card">
            <span class="overview-icon violet"><icon-link /></span>
            <small>{{ t("Codex 集成") }}</small>
            <strong>{{ t(snapshot?.initialized && selectedInstance?.openCodexConnected ? "已同步" : "未同步") }}</strong>
            <p>{{ t(snapshot?.initialized ? "配置与模型可同步" : "需要完成首次初始化") }}</p>
          </article>
        </section>

        <section class="quick-card">
          <div class="section-heading"><div><h2>{{ t("快捷操作") }}</h2><p>{{ t("所有命令均经过 Rust 白名单和参数校验") }}</p></div></div>
          <div class="quick-grid">
            <button :disabled="busy || !snapshot?.initialized" @click="run('doctor')"><span><icon-bug /></span><div><strong>{{ t("环境诊断") }}</strong><small>{{ t("检查运行环境与配置") }}</small></div><icon-right /></button>
            <button :disabled="busy || !snapshot?.initialized" @click="run('sync')"><span><icon-sync /></span><div><strong>{{ t("同步配置") }}</strong><small>{{ t("恢复基础配置，绑定 OAuth 并一键修复后打开实例") }}</small></div><icon-right /></button>
            <button :disabled="busy || selectionLocked" @click="requestOauthBinding">
              <span><icon-user /></span>
              <div>
                <strong>{{ t("绑定 OAuth") }}</strong>
                <small>{{ oauthBinding?.boundAccountId ? formatTranslatedText(t("当前：{account}，点击更换或取消绑定"), { account: oauthBinding.boundAccountLabel ?? oauthBinding.boundAccountId }) : t("为所选实例写入 ChatGPT 登录态") }}</small>
              </div>
              <icon-right />
            </button>
            <button
              :class="{ 'toggle-enabled': snapshot?.backgroundService?.installed }"
              :disabled="busy || !snapshot?.initialized || snapshot?.backgroundService?.supported === false"
              :title="backgroundServiceSummary()"
              @click="run(snapshot?.backgroundService?.installed ? 'service_uninstall' : 'service_install')"
            >
              <span><icon-storage /></span>
              <div>
                <strong>{{ t(snapshot?.backgroundService?.installed ? "取消后台服务" : "后台服务") }}</strong>
                <small>{{ t(snapshot?.backgroundService?.installed ? "已注册，再次点击取消" : "注册系统启动服务") }}</small>
              </div>
              <em :class="['quick-toggle-state', snapshot?.backgroundService?.installed ? 'on' : 'off']">
                {{ t(snapshot?.backgroundService?.installed ? "已开启" : "未开启") }}
              </em>
            </button>
            <button :disabled="busy || !snapshot?.initialized" @click="run('restore')"><span><icon-undo /></span><div><strong>{{ t("恢复 Codex") }}</strong><small>{{ t("仅还原所选实例配置") }}</small></div><icon-right /></button>
          </div>
        </section>

        <section class="console-card">
          <div class="section-heading">
            <div><h2>{{ t("最近输出") }}</h2><p>{{ t("凭据和 Token 会在进入前端前自动脱敏") }}</p></div>
            <a-button size="small" @click="page = 'logs'"><template #icon><icon-file /></template>{{ t("完整日志") }}</a-button>
          </div>
          <div class="console-output">
            <div v-if="!recentLogs.length" class="console-empty">{{ t("等待 OpenCodex 输出…") }}</div>
            <div v-for="(entry, index) in recentLogs" :key="`${entry.operationId}-${index}`" :class="['console-line', entry.stream]">
              <time>{{ formatLogTime(entry.timestamp) }}</time><span>{{ entry.line }}</span>
            </div>
          </div>
          <form v-if="interactiveOperationId" class="interactive-input" @submit.prevent="submitCommandInput">
            <a-input v-model="commandInput" autofocus :placeholder="t('输入初始化向导答案；直接回车表示使用默认值')" />
            <a-button html-type="submit" type="primary"><template #icon><icon-send /></template>{{ t("发送") }}</a-button>
          </form>
        </section>
      </template>

      <template v-else-if="page === 'web'">
        <section class="web-management-card">
          <div class="web-orb"><icon-public /></div>
          <a-tag :color="snapshot?.running ? 'green' : 'gray'">{{ t(snapshot?.running ? "服务已连接" : "服务未运行") }}</a-tag>
          <h2>OpenCodex Web Dashboard</h2>
          <p>{{ snapshot?.dashboardUrl || `http://127.0.0.1:${effectivePort}` }}</p>
          <div class="web-actions">
            <a-button type="primary" size="large" :disabled="!snapshot?.running" @click="openDashboard('client')"><template #icon><icon-desktop /></template>{{ t("客户端窗口打开") }}</a-button>
            <a-button size="large" :disabled="!snapshot?.running" @click="openDashboard('browser')"><template #icon><icon-launch /></template>{{ t("系统浏览器打开") }}</a-button>
          </div>
          <a-alert v-if="!snapshot?.running" type="warning" show-icon>{{ t("请先在“运行控制台”启动服务，再打开 Web 管理页面。") }}</a-alert>
        </section>
      </template>

      <template v-else-if="page === 'versions'">
        <section class="version-header-card">
          <div><i class="version-metric-icon"><icon-apps /></i><small>{{ t("已安装版本数") }}</small><strong>{{ catalog?.installedVersions.length ?? '—' }}</strong><span>{{ t("仅当前实例的本地版本") }}</span></div>
          <div><i class="version-metric-icon"><icon-clock-circle /></i><small>{{ t("当前运行版本") }}</small><strong>{{ snapshot?.engineVersion ? `v${snapshot.engineVersion}` : t("未安装") }}</strong><span>{{ t(snapshot?.running ? "运行中" : snapshot?.installed ? "服务已停止" : "请先下载 Engine") }}</span></div>
          <div><i class="version-metric-icon"><icon-download /></i><small>{{ t("最新稳定版") }}</small><strong>{{ catalog?.latestStable ? `v${catalog.latestStable.version}` : t("等待检测") }}</strong><span>GitHub Release</span></div>
        </section>
        <section class="version-manager-card">
          <div class="section-heading">
            <div><h2>{{ t("最新版本") }}</h2><p>{{ t("更新或切换后自动恢复服务运行状态，不影响其他实例。") }}</p></div>
            <a-button :loading="catalogLoading" @click="checkVersions"><template #icon><icon-refresh /></template>{{ t("检测更新") }}</a-button>
          </div>
          <div v-if="catalog?.latestStable" class="latest-release">
            <span class="release-icon"><icon-download /></span>
            <div><span class="release-status">{{ t(catalog.latestStable.newerThanCurrent ? "可用更新" : "最新稳定版") }}</span><h3>OpenCodex Engine v{{ catalog.latestStable.version }}</h3><p>{{ t("版本与数据目录独立，不影响其他实例。") }}</p></div>
            <a-button type="primary" :loading="installingVersion === catalog.latestStable.version" :disabled="accountMutationBusy || !catalog.latestStable.newerThanCurrent" @click="applyRelease(catalog.latestStable)">{{ t("更新到最新版") }}</a-button>
          </div>
          <div class="local-version-section">
            <div class="local-version-heading">
              <div><h3>{{ t("本地版本") }}</h3><p>{{ t("保留当前版本和最近 3 个历史版本，可随时切换或删除非当前版本。") }}</p></div>
              <span class="version-count">{{ catalog?.installedVersions.length || 0 }} {{ t("个版本") }}</span>
            </div>
            <div v-if="catalog?.installedVersions.length" class="local-version-list">
              <div class="local-version-table-heading"><span>{{ t("版本") }}</span><span>{{ t("状态") }}</span><span>{{ t("操作") }}</span></div>
              <div v-for="version in catalog.installedVersions" :key="version" class="local-version-row">
                <strong>Engine v{{ version }}</strong><span class="version-state" :class="{ current: catalog.currentVersion === version }">{{ t(catalog.currentVersion === version ? "当前使用" : "已安装") }}</span>
                <div class="local-version-actions">
                  <a-button size="small" :loading="installingVersion === version" :disabled="accountMutationBusy || catalog.currentVersion === version" @click="switchInstalledVersion(version)">{{ t(catalog.currentVersion === version ? "使用中" : "切换") }}</a-button>
                  <a-button size="small" class="version-delete" :aria-label="t('删除')" :title="t('删除版本')" :disabled="accountMutationBusy || (catalog.currentVersion === version && snapshot?.running)" @click="confirmDeleteInstalledVersion(version)"><template #icon><icon-delete /></template></a-button>
                </div>
              </div>
            </div>
            <a-empty v-else :description="t('暂无本地历史版本')" />
          </div>
          <div class="github-version-section">
            <div class="local-version-heading"><div><h3>{{ t("GitHub 版本") }}</h3><p>{{ t("选择官方 Release；未安装时下载并使用，已安装时直接切换。") }}</p></div></div>
          <div class="release-picker">
            <a-select v-model="selectedVersion" :placeholder="t('选择 Engine 版本')" :loading="catalogLoading" :disabled="accountMutationBusy">
              <a-option v-for="release in catalog?.releases || []" :key="release.version" :value="release.version">v{{ release.version }} · {{ t(release.prerelease ? "预览" : "稳定") }}{{ release.active ? ` · ${t("当前")}` : release.installed ? ` · ${t("已安装")}` : "" }}</a-option>
            </a-select>
            <a-button :loading="Boolean(selectedRelease && installingVersion === selectedRelease.version)" :disabled="accountMutationBusy || !selectedRelease || selectedRelease.active" @click="selectedRelease && applyRelease(selectedRelease)">{{ t(selectedRelease?.installed ? "切换版本" : "下载并使用") }}</a-button>
          </div>
          </div>
        </section>
      </template>

      <template v-else-if="page === 'vision'">
        <section v-if="!snapshot?.running" class="vision-offline-card">
          <span class="vision-offline-icon"><icon-image /></span>
          <h2>{{ t("启动服务后配置图片模型") }}</h2>
          <p>{{ t("模型列表来自当前运行中的 OpenCodex。启动后可选择需要通过图片描述器兼容的模型。") }}</p>
          <a-button type="primary" :disabled="busy || !snapshot?.initialized" @click="run('start')"><template #icon><icon-play-arrow /></template>{{ t("启动 OpenCodex") }}</a-button>
        </section>
        <section v-else class="vision-manager-card">
          <div class="vision-manager-header">
            <div>
              <span class="vision-kicker">VISION SIDECAR</span>
              <h2>{{ t("图片输入兼容") }}</h2>
              <p>{{ t("为本身不识图的模型开启图片粘贴。OpenCodex 会先用图片模型生成描述，再交给所选模型处理。") }}</p>
            </div>
          </div>
          <!-- 两张设置卡并排：左侧 Codex Image Gen 的转发上游，右侧图片描述器。 -->
          <div class="vision-editor-grid">
            <div class="vision-sidecar-editor image-gen-editor">
              <div class="vision-sidecar-editor-title">
                <div><small>{{ t("图片生成上游") }}</small><strong>{{ t("Codex Image Gen 转发目标") }}</strong></div>
                <a-button size="mini" :loading="imageGenLoading" :disabled="imageGenSaving" @click="loadImageGenerationSettings"><template #icon><icon-refresh /></template></a-button>
              </div>
              <label class="vision-sidecar-field" @click.prevent>
                <span>{{ t("提供方") }}</span>
                <a-select v-model="imageGenDraft.provider" :loading="imageGenLoading" :disabled="!imageGen || imageGenSaving" :placeholder="t('选择图片生成提供方')">
                  <a-option :value="IMAGE_GEN_DEFAULT_PROVIDER" :label="t('默认（OpenAI 上游）')">
                    <span class="image-gen-option"><span>{{ t("默认（OpenAI 上游）") }}</span><small>{{ t("ChatGPT 转发 / OpenAI API Key") }}</small></span>
                  </a-option>
                  <a-option
                    v-for="option in imageGenOptions"
                    :key="option.name"
                    :value="option.name"
                    :label="option.name"
                    :disabled="option.builtin || !option.hasApiKey"
                  >
                    <span class="image-gen-option"><span>{{ option.name }}</span><small>{{ option.builtin ? t("内置提供方") : !option.hasApiKey ? t("缺少 API Key") : option.adapter }}</small></span>
                  </a-option>
                </a-select>
              </label>
              <label class="vision-sidecar-field" @click.prevent>
                <span>{{ t("超时（秒）") }}</span>
                <a-input-number v-model="imageGenDraft.timeoutSeconds" :min="5" :max="300" :step="10" :disabled="!imageGen || imageGenSaving" />
              </label>
              <p :class="['vision-sidecar-hint', { warning: imageGenHintWarning }]"><icon-info-circle />{{ imageGenHint }}</p>
              <p class="vision-sidecar-hint"><icon-info-circle />{{ t("生图模型由 Codex 客户端决定（gpt-image 系列），所选提供方需提供对应模型。") }}</p>
              <div v-if="imageGenRecentRequests.length" class="image-gen-recent">
                <small>{{ t("最近生图请求") }}</small>
                <div v-for="request in imageGenRecentRequests" :key="`${request.timestamp}:${request.model}`" :class="['image-gen-recent-row', request.status >= 400 ? 'failed' : 'ok']">
                  <span class="image-gen-recent-status">{{ request.status }}</span>
                  <span class="image-gen-recent-model">{{ request.model }}</span>
                  <span class="image-gen-recent-meta">{{ request.errorCode ? request.errorCode : request.durationMs ? `${(request.durationMs / 1000).toFixed(1)}s` : '' }}</span>
                  <span class="image-gen-recent-time">{{ formatImageGenTime(request.timestamp) }}</span>
                </div>
                <p v-if="imageGenLatestFailed" class="vision-sidecar-hint warning"><icon-info-circle />{{ t("最近一次生图被上游拒绝：请求已正确转发到所选提供方，请确认该提供方的 /v1/images 接口与模型可用。") }}</p>
              </div>
              <p v-if="imageGenError" class="vision-sidecar-error">{{ imageGenError }}</p>
              <a-button type="primary" :loading="imageGenSaving" :disabled="!imageGen || imageGenLoading || busy" @click="saveImageGenerationSettings"><template #icon><icon-save /></template>{{ t("保存生图设置") }}</a-button>
            </div>
            <div class="vision-sidecar-editor">
              <div class="vision-sidecar-editor-title">
                <div><small>{{ t("图片描述模型") }}</small><strong>{{ t("直接修改 OpenCodex 描述器") }}</strong></div>
                <a-switch v-model="visionSidecarDraft.enabled" size="small" :aria-label="t('启用图片描述模型')" :disabled="!visionSidecar || visionSidecarSaving" />
              </div>
              <!--
                用 <label> 包裹 a-select 可保留隐式标签关联（arco 会把 aria-* attrs 落在 span 上，aria-labelledby 无效），
                但 label 的激活行为会把 click 再转发给内部 input，让不带 allow-search 的下拉刚开就关；
                所以用 @click.prevent 取消激活行为（不阻止冒泡，Trigger 仍能正常收到一次 click）。
              -->
              <label class="vision-sidecar-field" @click.prevent>
                <span>{{ t("模型") }}</span>
                <a-select
                  v-model="visionSidecarDraft.model"
                  :loading="visionLoading"
                  :disabled="!visionSidecar || visionSidecarSaving"
                  :placeholder="t('选择图片描述模型')"
                  allow-search
                  @change="selectVisionSidecarModel"
                >
                  <a-option v-for="option in visionSidecarModelOptions" :key="`${option.backend}:${option.value}`" :value="option.value">{{ option.label }}</a-option>
                </a-select>
              </label>
              <label class="vision-sidecar-field" @click.prevent>
                <span>{{ t("执行后端") }}</span>
                <a-select v-model="visionSidecarDraft.backend" :disabled="!visionSidecar || visionSidecarSaving">
                  <a-option
                    v-for="option in visionBackendOptions"
                    :key="option.value"
                    :value="option.value"
                    :disabled="visionSidecarDraft.model.includes('/') ? option.value !== 'routed' : option.value === 'routed'"
                  >{{ t(option.label) }}</a-option>
                </a-select>
              </label>
              <p class="vision-sidecar-hint"><icon-info-circle />{{ t(visionSidecarDraft.model.includes('/') ? "带 Provider 前缀的模型只能走路由转发" : "不带 Provider 前缀的模型需直连 OpenAI 或 Anthropic 接口") }}</p>
              <p v-if="visionSidecarError" class="vision-sidecar-error">{{ visionSidecarError }}</p>
              <a-button type="primary" :loading="visionSidecarSaving" :disabled="!visionSidecar || visionLoading || busy" @click="saveVisionSidecarSettings"><template #icon><icon-save /></template>{{ t("保存描述器") }}</a-button>
            </div>
          </div>
          <div class="vision-toolbar">
            <a-input v-model="visionSearch" allow-clear :placeholder="t('搜索 Provider 或模型名称')"><template #prefix><icon-search /></template></a-input>
            <a-button :loading="visionLoading" @click="loadVisionModels"><template #icon><icon-refresh /></template>{{ t("读取当前模型") }}</a-button>
            <a-button :disabled="!sidecarSelectableModels.length" @click="selectFilteredVisionModels">{{ t("选择当前结果") }}</a-button>
            <a-button :disabled="!selectedVisionCount" @click="clearVisionModels">{{ t("清空选择") }}</a-button>
          </div>
          <a-spin :loading="visionLoading" :tip="t('正在读取 OpenCodex 当前模型…')">
            <div v-if="filteredVisionModels.length" class="vision-model-list">
              <article
                v-for="model in filteredVisionModels"
                :key="model.namespaced"
                :class="['vision-model-row', { selected: selectedVisionModels.includes(model.namespaced), native: model.nativeVision, disabled: model.disabled }]"
                @click="!model.nativeVision && !model.disabled && toggleVisionModel(model.namespaced)"
              >
                <a-checkbox
                  v-if="!model.nativeVision"
                  :model-value="selectedVisionModels.includes(model.namespaced)"
                  :disabled="model.disabled"
                  @click.stop
                  @change="toggleVisionModel(model.namespaced)"
                />
                <span v-else class="vision-native-check"><icon-check /></span>
                <span class="vision-model-identity"><strong>{{ model.id }}</strong><small>{{ model.provider }}</small></span>
                <span v-if="model.nativeVision" class="vision-mode-pill native">{{ t("原生支持图片") }}</span>
                <span v-else-if="model.disabled" class="vision-mode-pill disabled">{{ t("模型已禁用") }}</span>
                <span v-else :class="['vision-mode-pill', selectedVisionModels.includes(model.namespaced) ? 'sidecar' : 'text']">{{ t(selectedVisionModels.includes(model.namespaced) ? '图片转文字' : '仅文本') }}</span>
              </article>
            </div>
            <a-empty v-else :description="t('没有匹配的模型')" />
          </a-spin>
          <footer class="vision-save-bar">
            <div><strong>{{ t("已选择") }} {{ selectedVisionCount }} {{ t("个兼容模型") }}</strong><span>{{ t("保存会自动重启 OpenCodex 并同步 Codex 模型目录") }}</span></div>
            <a-button type="primary" size="large" :loading="visionSaving" :disabled="visionLoading || busy" @click="saveVisionModels"><template #icon><icon-save /></template>{{ t("保存并同步") }}</a-button>
          </footer>
        </section>
      </template>

      <InstanceTransfer v-else-if="page === 'transfer'" :key="selectedInstanceId" :instances="instances" :target-id="selectedInstanceId" :disabled="selectionLocked" @busy="transferring = $event" @complete="refreshSnapshot()" />
      <template v-else-if="page === 'logs'">
        <section class="logs-card">
          <div class="section-heading">
            <div><h2>{{ t("运行日志") }}</h2><p>{{ t("最多保留并展示最近 1000 条脱敏输出") }}</p></div>
            <div><a-button size="small" @click="refreshSnapshot()"><template #icon><icon-refresh /></template>{{ t("刷新状态") }}</a-button><a-button size="small" @click="logs = []"><template #icon><icon-delete /></template>{{ t("清空显示") }}</a-button></div>
          </div>
          <div class="console-output full-log">
            <div v-if="!logs.length" class="console-empty">{{ t("暂无日志") }}</div>
            <div v-for="(entry, index) in logs" :key="`${entry.operationId}-${index}`" :class="['console-line', entry.stream]"><time>{{ formatLogTime(entry.timestamp) }}</time><span>{{ entry.line }}</span></div>
          </div>
          <form v-if="interactiveOperationId" class="interactive-input" @submit.prevent="submitCommandInput"><a-input v-model="commandInput" :placeholder="t('输入初始化向导答案')" /><a-button html-type="submit" type="primary">{{ t("发送") }}</a-button></form>
        </section>
      </template>

      <template v-else>
        <section class="opencodex-settings-grid">
          <div class="settings-side-column">
            <article class="settings-card service-settings">
              <header class="settings-card-head">
                <span class="settings-card-icon"><icon-settings /></span>
                <div><h2>{{ t("服务设置") }}</h2><p>{{ t("管理监听端口和 Dashboard 打开方式") }}</p></div>
              </header>
              <a-form :model="settings" layout="vertical">
                <a-form-item :label="t('服务端口')" :extra="t('修改后在下一次启动服务时生效')">
                  <a-input-number v-model="settings.port" :min="1024" :max="65535" :placeholder="String(DEFAULT_OPEN_CODEX_PORT)" />
                </a-form-item>
                <a-form-item :label="t('默认打开方式')" :extra="t('决定「访问地址」与 Web 管理默认如何打开 Dashboard')">
                  <a-radio-group v-model="settings.dashboardOpenMode" type="button">
                    <a-radio value="client"><icon-desktop />{{ t("客户端窗口") }}</a-radio>
                    <a-radio value="browser"><icon-launch />{{ t("系统浏览器") }}</a-radio>
                  </a-radio-group>
                </a-form-item>
              </a-form>
              <footer class="settings-card-foot">
                <span class="settings-current-hint">{{ t("当前生效端口") }} <code>{{ effectivePort }}</code></span>
                <a-button type="primary" @click="saveSettings"><template #icon><icon-save /></template>{{ t("保存设置") }}</a-button>
              </footer>
            </article>
            <article class="settings-card danger-card">
              <header class="settings-card-head">
                <span class="settings-card-icon warn"><icon-exclamation-circle /></span>
                <div><h2>{{ t("维护与恢复") }}</h2><p>{{ t("执行前请确认当前没有正在处理的请求") }}</p></div>
              </header>
              <div class="danger-list">
                <div class="danger-item">
                  <div><strong>{{ t("恢复原生 Codex") }}</strong><small>{{ t("仅还原所选实例的原生 Codex 配置，账号与数据保留") }}</small></div>
                  <a-button status="warning" :aria-label="t('恢复原生 Codex')" :disabled="busy || !snapshot?.initialized" @click="run('restore')"><template #icon><icon-undo /></template>{{ t("恢复") }}</a-button>
                </div>
                <div class="danger-item">
                  <div><strong>{{ t("卸载 OpenCodex") }}</strong><small>{{ t("恢复原生配置并移除此实例的 OpenCodex 接入") }}</small></div>
                  <a-button status="danger" :aria-label="t('卸载 OpenCodex')" :disabled="busy" @click="run('uninstall')"><template #icon><icon-delete /></template>{{ t("卸载") }}</a-button>
                </div>
              </div>
            </article>
          </div>
          <article class="settings-card migration-settings">
            <header class="settings-card-head">
              <span class="settings-card-icon"><icon-import /></span>
              <div><h2>{{ t("Switcher 账号迁移") }}</h2><p>{{ t("读取当前 Switcher 账号并转换为 OpenCodex OAuth 账号") }}</p></div>
              <a-button :loading="accountScanLoading" :disabled="accountMutationBusy" @click="scanAccounts"><template #icon><icon-refresh /></template>{{ t(accountScan ? "重新扫描" : "扫描") }}</a-button>
            </header>
            <div v-if="accountScan" class="scan-summary">
              <span class="scan-stat"><small>{{ t("发现") }}</small><strong>{{ accountScan.totalCount }}</strong><em>{{ t("个账号") }}</em></span>
              <span class="scan-stat scan-ready"><small>{{ t("可导入") }}</small><strong>{{ accountScan.eligibleCount }}</strong><em>{{ t("个") }}</em></span>
              <span class="scan-stat scan-selected"><small>{{ t("已选择") }}</small><strong>{{ selectedAccountIds.length }}</strong><em>{{ t("个") }}</em></span>
            </div>
            <div class="migration-toolbar">
              <a-input v-model="accountSearch" allow-clear :placeholder="t('搜索邮箱或账号 ID')" :aria-label="t('搜索邮箱或账号 ID')"><template #prefix><icon-search /></template></a-input>
              <a-select v-model="accountStatus" :aria-label="t('账号状态')">
                <a-option value="">{{ t("全部状态") }}</a-option>
                <a-option v-for="status in ['ready', 'already_imported', 'unsupported', 'invalid']" :key="status" :value="status">{{ accountStatusLabel(status) }}</a-option>
              </a-select>
              <a-select v-model="accountPlan" :aria-label="t('套餐')">
                <a-option value="">{{ t("全部套餐") }}</a-option>
                <a-option v-for="plan in accountPlans" :key="plan" :value="plan">{{ plan === '__unknown__' ? t('未知套餐') : plan }}</a-option>
              </a-select>
            </div>
            <div v-if="accountScan" class="migration-selection-bar">
              <a-checkbox :model-value="allVisibleSelected" :indeterminate="visibleSelectedCount > 0 && !allVisibleSelected" :disabled="!visibleSelectable.length || accountMutationBusy || accountScanLoading" @change="selectVisibleAccounts">{{ t("全选筛选结果") }}</a-checkbox>
              <span>{{ formatTranslatedText("显示 {visible} / {total} 个账号", { visible: filteredAccounts.length, total: accountScan.totalCount }) }}</span>
              <span v-if="hiddenSelectedCount" class="hidden-selection">{{ formatTranslatedText("另有 {count} 个已选账号被筛选隐藏", { count: hiddenSelectedCount }) }}</span>
              <a-button type="text" size="small" :disabled="!selectedAccountIds.length || accountMutationBusy || accountScanLoading" @click="selectedAccountIds = []">{{ t("清空选择") }}</a-button>
            </div>
            <a-spin :loading="accountScanLoading" class="migration-results">
              <div v-if="filteredAccounts.length" class="migration-table-scroll">
                <table class="migration-table">
                  <thead><tr><th class="migration-check-column"><span class="sr-only">{{ t("选择") }}</span></th><th>{{ t("账号") }}</th><th>{{ t("套餐") }}</th><th>{{ t("状态") }}</th><th>{{ t("操作") }}</th></tr></thead>
                  <tbody>
                    <tr v-for="account in filteredAccounts" :key="account.sourceId" :class="{ selected: selectedAccountIds.includes(account.sourceId) }">
                      <td><a-checkbox :model-value="selectedAccountIds.includes(account.sourceId)" :disabled="accountMutationBusy || accountScanLoading || (!account.eligible && !account.deletable)" :aria-label="account.email || account.sourceId" @change="toggleMigrationAccount(account)" /></td>
                      <td class="migration-identity">
                        <div><a-tooltip :content="account.email || account.sourceId"><strong>{{ account.email || account.sourceId }}</strong></a-tooltip><a-tag v-if="account.current" size="small" color="blue">{{ t("当前") }}</a-tag></div>
                        <a-tooltip :content="account.sourceId"><small>{{ account.sourceId }}</small></a-tooltip>
                      </td>
                      <td class="migration-plan"><a-tooltip :content="account.plan || t('未知套餐')"><span>{{ account.plan || t("未知套餐") }}</span></a-tooltip></td>
                      <td><a-tooltip :content="accountStatusLabel(account.status)"><span :class="['migration-status-pill', account.status]">{{ accountStatusLabel(account.status) }}</span></a-tooltip><a-tooltip :content="t(account.reason)"><p class="migration-reason">{{ t(account.reason) }}</p></a-tooltip></td>
                      <td><a-tooltip v-if="account.deletable" :content="t('从 OpenCodex 删除，保留 Switcher 原账号')"><a-button type="text" size="small" status="danger" :loading="deletingAccountId === account.sourceId" :disabled="accountMutationBusy || snapshot?.running" :aria-label="t('删除 OpenCodex 账号')" @click="confirmDeleteMigratedAccount(account)"><template #icon><icon-delete /></template></a-button></a-tooltip><span v-else class="migration-no-action">-</span></td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <div v-else class="migration-empty">
                <span class="migration-empty-icon"><icon-user-group /></span>
                <strong>{{ t(!accountScan ? '尚未扫描账号' : !accountScan.accounts.length ? '暂无账号' : '没有符合筛选条件的账号') }}</strong>
                <p v-if="!accountScan">{{ t("点击右上角「扫描」读取 Switcher 中的 OAuth 账号，选择后即可导入到当前实例。") }}</p>
              </div>
            </a-spin>
            <footer v-if="accountScan" class="migration-batch-actions">
              <a-alert v-if="snapshot?.running" type="warning" show-icon>{{ t("导入前请先停止 OpenCodex 服务，避免配置被运行中的 Engine 覆盖。") }}</a-alert>
              <span v-else class="migration-batch-hint">{{ t("导入的账号会保留在 Switcher 中，删除仅影响 OpenCodex 侧副本。") }}</span>
              <div>
                <a-button status="danger" :loading="deletingAccountId === '__selected__'" :disabled="accountMutationBusy || accountScanLoading || !selectedDeleteAccounts.length || snapshot?.running" @click="confirmDeleteSelectedAccounts"><template #icon><icon-delete /></template>{{ t("删除所选") }}（{{ selectedDeleteAccounts.length }}）</a-button>
                <a-button type="primary" :loading="importingAccounts" :disabled="accountMutationBusy || accountScanLoading || !selectedImportAccountIds.length || snapshot?.running" @click="importAccounts"><template #icon><icon-import /></template>{{ t("导入所选") }}（{{ selectedImportAccountIds.length }}）</a-button>
              </div>
            </footer>
          </article>
        </section>
      </template>
    </a-spin>

  </section>
</template>

<style scoped>
.instance-storage-hint { display: flex; justify-content: flex-end; gap: 10px; align-items: baseline; margin: -6px 0 0; color: #98a2b3; font-size: 12px; }
.instance-storage-hint > span { flex-shrink: 0; }
.instance-storage-hint code { color: #667085; overflow-wrap: anywhere; font-size: 11px; }
.opencodex-hero { flex-wrap: wrap; }
.instance-target-bar { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin-left: auto; min-width: 0; }
.instance-target-bar label { white-space: nowrap; font-size: 12px; color: #718096; }
.service-control-card { flex-wrap: wrap; }
.opencodex-page { display: grid; grid-template-columns: minmax(0, 1fr); min-width: 0; gap: 18px; color: #101827; container-type: inline-size; }
.opencodex-page > * { min-width: 0; }
.opencodex-hero { display: flex; align-items: center; justify-content: space-between; gap: 16px 24px; }
.opencodex-hero > .opencodex-status-strip { flex-basis: 100%; }
.opencodex-title-row { display: flex; align-items: center; gap: 14px; }
.opencodex-brand-mark { display: grid; flex-shrink: 0; width: 44px; height: 44px; place-items: center; border-radius: 10px; color: #2563eb; background: #edf3ff; font-size: 16px; font-weight: 800; }
.opencodex-title-row h1 { margin: 0; font-size: 24px; letter-spacing: 0; overflow-wrap: anywhere; }
.opencodex-title-row p { margin: 5px 0 0; color: #667085; font-size: 13px; }
.opencodex-status-strip { display: flex; min-height: 36px; align-items: center; flex-wrap: wrap; gap: 12px; color: #667085; font-size: 12px; }
.opencodex-status-strip > span:not(.status-dot) { padding-left: 16px; border-left: 1px solid rgba(85, 113, 156, .14); }
.status-dot { width: 10px; height: 10px; border-radius: 50%; }.status-dot.online { background: #16a34a; box-shadow: 0 0 0 5px rgba(22, 163, 74, .12); }.status-dot.offline { background: #94a3b8; }
.opencodex-tabs { display: flex; overflow-x: auto; gap: 4px; padding: 0; border-bottom: 1px solid #dce1e9; }
.opencodex-tabs button { display: flex; flex: 0 0 auto; min-height: 44px; align-items: center; gap: 8px; padding: 0 16px; border: 0; border-bottom: 2px solid transparent; color: #667085; background: transparent; font-size: 13px; font-weight: 600; cursor: pointer; }
.opencodex-tabs button.active { color: #2563eb; border-bottom-color: #2563eb; }
.opencodex-tabs button:hover { color: #2563eb; background: #f5f8ff; }
.opencodex-tabs button:focus-visible { outline: 2px solid #2563eb; outline-offset: -2px; }
.engine-setup-notice { display: flex; align-items: center; gap: 12px; padding: 12px 16px; border: 1px solid #e4e7ec; border-radius: 8px; background: #fff; color: #667085; }
.engine-setup-notice > div { display: flex; flex: 1; flex-wrap: wrap; align-items: baseline; gap: 6px 12px; font-size: 12px; }
.engine-setup-notice strong { color: #475467; font-size: 13px; }
.engine-setup-notice > .arco-btn { flex-shrink: 0; }
.opencodex-content { display: block; min-height: 460px; }
.service-control-card, .quick-card, .console-card, .version-manager-card, .logs-card, .web-management-card { border: 1px solid rgba(85, 113, 156, .17); border-radius: 16px; background: rgba(255, 255, 255, .82); box-shadow: 0 14px 34px rgba(30, 53, 84, .06); }
.service-control-card { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 16px 18px; }.service-actions { display: flex; flex-wrap: wrap; gap: 10px; }.health-indicator { display: flex; align-items: center; gap: 10px; color: #0f766e; }.health-indicator span { display: grid; }.health-indicator small { color: #718096; }.health-indicator strong { font-size: 15px; }
.overview-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 14px; margin-top: 16px; }.overview-card { position: relative; min-width: 0; padding: 20px; border: 1px solid rgba(85, 113, 156, .17); border-radius: 15px; background: rgba(255, 255, 255, .8); box-shadow: 0 12px 28px rgba(30, 53, 84, .05); }.overview-card > small { display: block; color: #66758c; font-weight: 700; }.overview-card > strong { display: block; overflow: hidden; margin-top: 9px; font-size: 21px; text-overflow: ellipsis; white-space: nowrap; }.overview-card > p { display: flex; gap: 12px; margin: 9px 0 0; color: #78879a; }.overview-icon { position: absolute; top: 17px; right: 17px; display: grid; width: 38px; height: 38px; place-items: center; border-radius: 12px; }.overview-icon.green { color: #07866f; background: #e1f5ef; }.overview-icon.blue { color: #2563eb; background: #e9f1ff; }.overview-icon.cyan { color: #0891b2; background: #e3f7fb; }.overview-icon.violet { color: #7c3aed; background: #f1eaff; }
.quick-card, .console-card, .version-manager-card, .logs-card { margin-top: 16px; padding: 18px; }.section-heading { display: flex; align-items: center; justify-content: space-between; gap: 14px; margin-bottom: 14px; }.section-heading h2 { margin: 0; font-size: 18px; }.section-heading p { margin: 4px 0 0; color: #718096; }.quick-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }.quick-grid button { display: flex; min-width: 0; align-items: center; gap: 13px; padding: 14px; border: 1px solid rgba(85, 113, 156, .16); border-radius: 12px; color: #172033; background: rgba(249, 252, 255, .86); text-align: left; cursor: pointer; }.quick-grid button:hover:not(:disabled) { border-color: rgba(15, 118, 110, .3); background: #f0fbf8; }.quick-grid button:disabled { opacity: .48; cursor: not-allowed; }.quick-grid button > span { display: grid; width: 38px; height: 38px; flex: 0 0 auto; place-items: center; border-radius: 11px; color: #0f766e; background: #e7f6f3; }.quick-grid button > div { display: grid; flex: 1; }.quick-grid small { margin-top: 3px; color: #718096; }
.quick-grid button.toggle-enabled { border-color: rgba(16, 185, 129, .24); background: rgba(240, 253, 250, .88); }.quick-toggle-state { display: inline-flex; flex: 0 0 auto; height: 24px; align-items: center; padding: 0 9px; border-radius: 999px; font-size: 10px; font-style: normal; font-weight: 820; }.quick-toggle-state.on { color: #047857; background: #d1fae5; }.quick-toggle-state.off { color: #64748b; background: #eef2f7; }
.console-output { overflow: auto; max-height: 280px; min-height: 150px; padding: 15px; border-radius: 12px; background: #111821; color: #b9f69b; font: 12.5px/1.65 ui-monospace, SFMono-Regular, Menlo, monospace; }.console-line { display: grid; grid-template-columns: 72px minmax(0, 1fr); gap: 8px; }.console-line time { color: #64748b; }.console-line.stderr span { color: #fda4af; }.console-line.system span { color: #93c5fd; }.console-empty { display: grid; min-height: 120px; place-items: center; color: #64748b; }.interactive-input { display: flex; gap: 10px; margin-top: 12px; }
.web-management-card { display: grid; justify-items: center; padding: 58px 28px; text-align: center; }.web-orb { display: grid; width: 82px; height: 82px; place-items: center; margin-bottom: 18px; border-radius: 26px; color: #0f766e; background: #e3f6f1; font-size: 34px; }.web-management-card h2 { margin: 14px 0 6px; font-size: 28px; }.web-management-card > p { color: #66758c; font: 16px ui-monospace, SFMono-Regular, Menlo, monospace; }.web-actions { display: flex; gap: 12px; margin: 24px 0; }
.vision-offline-card { display: grid; min-height: 430px; place-items: center; align-content: center; gap: 12px; padding: 48px 24px; border: 1px solid rgba(85, 113, 156, .17); border-radius: 16px; background: rgba(255, 255, 255, .82); text-align: center; }.vision-offline-card h2 { margin: 8px 0 0; font-size: 24px; }.vision-offline-card p { max-width: 560px; margin: 0 0 8px; color: #66758c; line-height: 1.7; }.vision-offline-icon { display: grid; width: 72px; height: 72px; place-items: center; border-radius: 20px; color: #0f766e; background: #e3f6f1; font-size: 30px; }
.vision-manager-card { overflow: hidden; border: 1px solid rgba(85, 113, 156, .17); border-radius: 16px; background: rgba(255, 255, 255, .86); box-shadow: 0 14px 34px rgba(30, 53, 84, .06); }.vision-manager-header { display: grid; grid-template-columns: minmax(0, 1fr) minmax(210px, 280px); align-items: center; gap: 24px; padding: 24px; border-bottom: 1px solid rgba(85, 113, 156, .13); background: linear-gradient(120deg, rgba(238, 250, 247, .94), rgba(242, 247, 255, .94)); }.vision-kicker { color: #0f766e; font-size: 11px; font-weight: 850; letter-spacing: .08em; }.vision-manager-header h2 { margin: 6px 0; font-size: 24px; }.vision-manager-header p { max-width: 700px; margin: 0; color: #607087; line-height: 1.65; }
.vision-toolbar { display: grid; grid-template-columns: minmax(220px, 1fr) auto auto auto; gap: 8px; padding: 16px 18px; border-bottom: 1px solid rgba(85, 113, 156, .12); }.vision-model-list { display: grid; max-height: min(52vh, 560px); overflow-y: auto; padding: 8px 18px 18px; }.vision-model-row { display: grid; grid-template-columns: 34px minmax(0, 1fr) auto; min-width: 0; align-items: center; gap: 10px; min-height: 58px; padding: 8px 10px; border-bottom: 1px solid rgba(85, 113, 156, .1); cursor: pointer; }.vision-model-row:hover:not(.disabled):not(.native), .vision-model-row.selected { background: rgba(236, 253, 245, .74); }.vision-model-row.native { cursor: default; }.vision-model-row.disabled { opacity: .52; cursor: not-allowed; }.vision-native-check { display: grid; width: 22px; height: 22px; place-items: center; border-radius: 50%; color: #fff; background: #16a34a; font-size: 12px; }.vision-model-identity { display: grid; min-width: 0; gap: 3px; }.vision-model-identity strong, .vision-model-identity small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.vision-model-identity strong { color: #172033; font-size: 14px; }.vision-model-identity small { color: #718096; font-size: 11px; }.vision-mode-pill { display: inline-flex; min-width: 76px; height: 25px; align-items: center; justify-content: center; padding: 0 9px; border-radius: 999px; font-size: 10px; font-weight: 820; }.vision-mode-pill.native { color: #047857; background: #d1fae5; }.vision-mode-pill.sidecar { color: #1d4ed8; background: #dbeafe; }.vision-mode-pill.text { color: #64748b; background: #eef2f7; }.vision-mode-pill.disabled { color: #9f1239; background: #ffe4e6; }.vision-save-bar { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 16px 20px; border-top: 1px solid rgba(85, 113, 156, .14); background: #f8fafc; }.vision-save-bar > div { display: grid; gap: 3px; }.vision-save-bar span { color: #718096; font-size: 12px; }
.version-header-card { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); overflow: hidden; border: 1px solid rgba(85, 113, 156, .17); border-radius: 16px; background: rgba(255, 255, 255, .82); }.version-header-card > div { display: grid; gap: 6px; padding: 22px; }.version-header-card > div + div { border-left: 1px solid rgba(85, 113, 156, .14); }.version-header-card small, .version-header-card span { color: #718096; }.version-header-card strong { font-size: 26px; }.latest-release { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 16px; padding: 20px; border-radius: 14px; background: linear-gradient(135deg, #effaf7, #eef6ff); }.release-icon { display: grid; width: 58px; height: 58px; place-items: center; border-radius: 18px; color: #0f766e; background: #fff; font-size: 24px; }.latest-release h3 { margin: 8px 0 4px; font-size: 21px; }.latest-release p { margin: 0; color: #66758c; }.release-picker { display: flex; gap: 10px; margin-top: 14px; }.release-picker .arco-select { flex: 1; }
.local-version-section, .github-version-section { margin-top: 18px; padding-top: 18px; border-top: 1px solid rgba(85, 113, 156, .14); }.local-version-heading { display: flex; align-items: center; justify-content: space-between; gap: 14px; }.local-version-heading h3 { margin: 0; font-size: 16px; }.local-version-heading p { margin: 4px 0 0; color: #718096; }.local-version-list { display: grid; gap: 8px; margin-top: 12px; }.local-version-row { display: flex; min-width: 0; align-items: center; justify-content: space-between; gap: 12px; padding: 11px 12px; border: 1px solid rgba(85, 113, 156, .14); border-radius: 10px; background: rgba(248, 251, 255, .86); }.local-version-row > div:first-child { display: grid; gap: 3px; min-width: 0; }.local-version-row span { color: #718096; font-size: 12px; }.local-version-actions { display: flex; flex: 0 0 auto; gap: 6px; }
.full-log { max-height: calc(100vh - 330px); min-height: 420px; }
.engine-progress { padding: 14px 0; border-block: 1px solid #e5e7eb; }
.engine-progress-heading { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; margin-bottom: 8px; font-size: 13px; }
.engine-progress-heading span { color: #6b7280; }
.engine-indeterminate { overflow: hidden; }
.engine-indeterminate :deep(.arco-progress-line-bar) { animation: engine-pending 1.5s ease-in-out infinite; }
.engine-error { margin: 0; color: #c4233b; white-space: pre-wrap; overflow-wrap: anywhere; }
@keyframes engine-pending { from { transform: translateX(-100%); } to { transform: translateX(340%); } }
@media (prefers-reduced-motion: reduce) { .engine-indeterminate :deep(.arco-progress-line-bar) { animation-duration: 4s; } }
.version-manager-card { border: 1px solid #e4e7ec; border-radius: 12px; box-shadow: none; background: #fff; padding: 24px; }
.version-manager-card .section-heading { margin-bottom: 20px; }
.version-manager-card .section-heading h2 { font-size: 16px; }
.version-manager-card .section-heading p, .local-version-heading p, .latest-release p { font-size: 12px; line-height: 1.6; color: #667085; }
.version-manager-card .section-heading > .arco-btn { flex-shrink: 0; }
.version-header-card { margin-top: 0; gap: 16px; border: 0; border-radius: 0; background: transparent; }
.version-header-card > div { position: relative; padding: 20px 16px 20px 76px; border: 1px solid #e4e7ec; border-radius: 10px; background: #fff; }
.version-header-card > div + div { border: 1px solid #e4e7ec; }
.version-metric-icon { position: absolute; left: 18px; top: 20px; display: grid; place-items: center; width: 40px; height: 40px; border-radius: 8px; color: #2563eb; background: #edf3ff; font-size: 22px; }
.version-header-card small, .version-header-card span { font-size: 12px; color: #667085; }
.version-header-card strong { font-size: 22px; font-variant-numeric: tabular-nums; }
.latest-release { padding: 16px; background: #f8faff; border: 1px solid #e1e9f8; border-radius: 8px; }
.release-icon { width: 40px; height: 40px; border-radius: 8px; color: #2563eb; background: #edf3ff; font-size: 22px; }
.release-status { color: #2563eb; font-size: 12px; font-weight: 600; }
.latest-release h3 { margin: 4px 0; font-size: 17px; }
.local-version-section, .github-version-section { margin-top: 24px; padding-top: 0; border-top: 0; }
.github-version-section { padding-top: 20px; border-top: 1px solid #eef0f3; }
.local-version-heading h3 { font-size: 14px; }
.version-count { flex-shrink: 0; color: #667085; font-size: 12px; }
.local-version-list { gap: 0; overflow: hidden; border: 1px solid #e4e7ec; border-radius: 8px; }
.local-version-row, .local-version-table-heading { display: grid; grid-template-columns: minmax(0, 1fr) 110px 156px; align-items: center; gap: 16px; padding: 12px 16px; }
.local-version-table-heading { background: #f8f9fb; color: #667085; font-size: 12px; }
.local-version-table-heading > :last-child { text-align: right; }
.local-version-row { min-height: 58px; border: 0; border-top: 1px solid #eef0f3; border-radius: 0; background: #fff; }
.local-version-row > strong { font-size: 13px; font-weight: 600; overflow-wrap: anywhere; }
.local-version-row .version-state { justify-self: start; color: #667085; font-size: 12px; }
.local-version-row .version-state.current { color: #16805b; }
.version-state::before { content: ''; display: inline-block; width: 6px; height: 6px; margin-right: 6px; border-radius: 50%; background: #98a2b3; vertical-align: 1px; }
.version-state.current::before { background: #16a36b; }
.local-version-actions { justify-content: flex-end; gap: 8px; }
.release-picker { display: grid; grid-template-columns: minmax(0, 1fr) 156px; gap: 16px; }
.opencodex-page :deep(.arco-btn) { border-radius: 6px; }
.service-actions :deep(.arco-btn), .version-manager-card :deep(.arco-btn) { height: 36px; }
.local-version-actions :deep(.arco-btn) { height: 30px; }
.local-version-actions :deep(.arco-btn:not(.version-delete)) { min-width: 72px; }
.local-version-actions :deep(.arco-btn:not(.arco-btn-disabled)), .version-manager-card :deep(.arco-btn-secondary) { border: 1px solid #d0d5dd; background: #fff; }
.local-version-actions :deep(.version-delete:not(.arco-btn-disabled):hover) { color: #d92d20; border-color: #fda29b; background: #fff6f5; }
.instance-target-bar :deep(.arco-select-view), .release-picker :deep(.arco-select-view) { background: #fff; border: 1px solid #d0d5dd; border-radius: 6px; }
.service-control-card { padding: 12px 16px; border-radius: 10px; background: #fff; border-color: #e4e7ec; box-shadow: none; }
.health-indicator { color: #98a2b3; font-size: 12px; }
.health-indicator.healthy { color: #16805b; }
.health-indicator strong { font-size: 13px; }
.overview-card, .quick-card, .console-card { border-radius: 10px; box-shadow: none; background: #fff; border-color: #e4e7ec; }
.overview-icon.green, .overview-icon.cyan, .overview-icon.violet, .quick-grid button > span { color: #2563eb; background: #edf3ff; }
.quick-grid button:hover:not(:disabled) { border-color: #93b4f8; background: #f5f8ff; }
/* ===== 设置页：三张卡片（服务设置 / 维护与恢复 | 账号迁移） ===== */
.opencodex-settings-grid { display: grid; grid-template-columns: minmax(300px, 360px) minmax(0, 1fr); align-items: start; gap: 16px; }
.settings-side-column { display: grid; gap: 16px; min-width: 0; }
.settings-card { display: flex; flex-direction: column; min-width: 0; overflow: hidden; border: 1px solid var(--oc-line); border-radius: 10px; background: var(--oc-surface); box-shadow: 0 12px 30px rgba(20, 35, 55, .06); backdrop-filter: blur(14px); }
.settings-card > * { flex: 0 0 auto; }
.settings-card-head { display: flex; align-items: center; gap: 12px; padding: 16px 18px; border-bottom: 1px solid var(--oc-line); background: linear-gradient(180deg, #fbfcfe, #f6f8fc); }
.settings-card-head > div { display: grid; flex: 1 1 auto; gap: 3px; min-width: 0; }
.settings-card-head h2 { margin: 0; color: var(--oc-ink); font-size: 15px; font-weight: 700; overflow-wrap: anywhere; }
.settings-card-head p { margin: 0; color: var(--oc-muted); font-size: 12px; line-height: 1.5; }
.settings-card-head > .arco-btn { flex: 0 0 auto; }
.settings-card-icon { display: grid; width: 38px; height: 38px; flex: 0 0 auto; place-items: center; border-radius: 10px; color: var(--oc-accent); background: var(--oc-accent-soft); font-size: 19px; }
.settings-card-icon.warn { color: #b54708; background: #fff4e5; }
.service-settings :deep(.arco-form) { padding: 18px 18px 4px; }
.service-settings :deep(.arco-form-item) { margin-bottom: 18px; }
.service-settings :deep(.arco-form-item-label) { color: var(--oc-ink); font-size: 13px; font-weight: 600; }
.service-settings :deep(.arco-form-item-extra) { color: var(--oc-muted); font-size: 12px; }
.service-settings :deep(.arco-input-number) { width: 100%; }
.service-settings :deep(.arco-input-number .arco-input-wrapper), .service-settings :deep(.arco-radio-group-button) { border-radius: 6px; }
.service-settings :deep(.arco-radio-group-button) { display: flex; width: 100%; }
.service-settings :deep(.arco-radio-button) { flex: 1; text-align: center; }
.service-settings :deep(.arco-radio-button-content) { display: inline-flex; align-items: center; gap: 6px; }
.settings-card-foot { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 10px; padding: 12px 18px; border-top: 1px solid var(--oc-line); background: var(--oc-soft); }
.settings-current-hint { color: var(--oc-muted); font-size: 12px; }
.settings-current-hint code { margin-left: 4px; padding: 1px 6px; border-radius: 4px; color: var(--oc-ink); background: #fff; border: 1px solid var(--oc-line); font-size: 12px; }
.danger-list { display: grid; }
.danger-item { display: flex; align-items: center; justify-content: space-between; gap: 14px; padding: 14px 18px; }
.danger-item + .danger-item { border-top: 1px solid var(--oc-line); }
.danger-item > div { display: grid; gap: 3px; min-width: 0; }
.danger-item strong { color: var(--oc-ink); font-size: 13px; font-weight: 600; }
.danger-item small { color: var(--oc-muted); font-size: 12px; line-height: 1.5; }
.danger-item > .arco-btn { flex: 0 0 auto; }
.migration-settings { align-self: stretch; }
.scan-summary { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; padding: 14px 18px 0; }
.scan-stat { display: flex; align-items: baseline; flex-wrap: wrap; gap: 4px 6px; padding: 10px 12px; border: 1px solid var(--oc-line); border-radius: 8px; background: var(--oc-soft); color: var(--oc-muted); font-size: 12px; }
.scan-stat small { flex-basis: 100%; font-size: 11px; }
.scan-stat strong { color: var(--oc-ink); font-size: 20px; font-weight: 700; line-height: 1; font-variant-numeric: tabular-nums; }
.scan-stat em { font-style: normal; }
.scan-stat.scan-ready { border-color: #bfe8d3; background: #f0fbf5; }
.scan-stat.scan-ready strong { color: #12805c; }
.scan-stat.scan-selected { border-color: #c7dbfb; background: #f3f7ff; }
.scan-stat.scan-selected strong { color: var(--oc-accent); }
.migration-toolbar { display: grid; grid-template-columns: minmax(0, 1.6fr) minmax(0, 1fr) minmax(0, 1fr); gap: 8px; padding: 14px 18px 0; }
.migration-toolbar :deep(.arco-input-wrapper), .migration-toolbar :deep(.arco-select-view) { border-radius: 6px; }
.migration-selection-bar { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 10px; padding: 10px 18px 6px; color: var(--oc-muted); font-size: 12px; }
.migration-selection-bar :deep(.arco-checkbox-label) { font-size: 12px; }
.migration-selection-bar > .arco-btn { margin-left: auto; padding-inline: 4px; }
.hidden-selection { color: #ad6510; }
.migration-settings .migration-results { display: flex; flex: 1 1 auto; flex-direction: column; min-height: 220px; }
.migration-results > .migration-empty { flex: 1 1 auto; align-content: center; }
.migration-table-scroll { max-height: min(46vh, 480px); overflow: auto; border-block: 1px solid var(--oc-line); }
.migration-empty { display: grid; justify-items: center; gap: 6px; padding: 40px 24px; text-align: center; }
.migration-empty-icon { display: grid; width: 52px; height: 52px; place-items: center; margin-bottom: 6px; border-radius: 16px; color: var(--oc-accent); background: var(--oc-accent-soft); font-size: 24px; }
.migration-empty strong { color: var(--oc-ink); font-size: 14px; }
.migration-empty p { max-width: 380px; margin: 0; color: var(--oc-muted); font-size: 12px; line-height: 1.6; }
.migration-table { width: 100%; min-width: 340px; border-collapse: separate; border-spacing: 0; table-layout: fixed; font-size: 13px; text-align: left; }
.migration-table th { position: sticky; top: 0; z-index: 1; padding: 8px 10px; background: #f6f8fc; color: var(--oc-muted); font-size: 12px; font-weight: 600; }
.migration-table th:first-child, .migration-table td:first-child { padding-left: 18px; }
.migration-table th:last-child, .migration-table td:last-child { padding-right: 18px; }
.migration-table th:first-child { width: 44px; }
.migration-table th:nth-child(3) { width: 72px; }
.migration-table th:nth-child(4) { width: 28%; }
.migration-table th:last-child { width: 52px; }
.migration-table td { padding: 8px 10px; border-top: 1px solid #eef0f3; vertical-align: middle; background: #fff; }
.migration-table tbody tr:hover td { background: #f8fafc; }
.migration-table tr.selected td { background: #f0f6ff; }
.migration-identity > div { display: flex; align-items: center; gap: 4px; }
.migration-identity strong { display: block; min-width: 0; font-weight: 500; color: #1d2939; line-height: 18px; }
.migration-identity :deep(.arco-tag) { flex: 0 0 auto; }
.migration-identity small { display: block; margin-top: 2px; color: #86909c; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 11px; line-height: 16px; }
.migration-plan > span { display: block; color: #667085; font-size: 12px; }
.migration-reason { margin: 2px 0 0; color: #667085; font-size: 12px; line-height: 16px; }
.migration-identity strong, .migration-identity small, .migration-plan > span, .migration-reason, .migration-status-pill { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.migration-status-pill { display: inline-block; max-width: 100%; padding: 1px 5px; border-radius: 4px; background: #f2f3f5; color: #667085; font-size: 12px; line-height: 18px; vertical-align: middle; }
.migration-status-pill.ready { color: #12805c; background: #e8f7ee; }
.migration-status-pill.already_imported { color: #245cb9; background: #eaf1ff; }
.migration-status-pill.unsupported { color: #ad6510; background: #fff5df; }
.migration-status-pill.invalid { color: #c4233b; background: #fff0f0; }
.migration-no-action { color: #c0c4cc; }
.migration-batch-actions { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 10px; padding: 12px 18px; border-top: 1px solid var(--oc-line); background: var(--oc-soft); }
.migration-batch-actions > div { display: flex; flex-wrap: wrap; gap: 8px; margin-left: auto; }
.migration-batch-hint { color: var(--oc-muted); font-size: 12px; }
.migration-settings :deep(.arco-alert) { flex: 1 1 260px; padding: 6px 10px; font-size: 12px; border-radius: 6px; }
.sr-only { position: absolute; width: 1px; height: 1px; padding: 0; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; }
@container (max-width: 900px) { .opencodex-settings-grid { grid-template-columns: minmax(0, 1fr); } .settings-side-column { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
@container (max-width: 640px) { .settings-side-column { grid-template-columns: minmax(0, 1fr); } .scan-summary { grid-template-columns: minmax(0, 1fr); } .migration-toolbar { grid-template-columns: repeat(2, minmax(0, 1fr)); } .migration-toolbar > :first-child { grid-column: 1 / -1; } .danger-item { flex-direction: column; align-items: stretch; } }
@media (max-width: 1080px) { .overview-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
@media (max-width: 760px) { .opencodex-tabs { overflow-x: auto; }.opencodex-tabs button { flex: 0 0 auto; }.overview-grid, .quick-grid, .version-header-card, .vision-manager-header { grid-template-columns: 1fr; }.version-header-card > div + div { border-top: 1px solid rgba(85, 113, 156, .14); border-left: 0; }.latest-release { grid-template-columns: 1fr; }.release-picker, .web-actions { align-items: stretch; flex-direction: column; }.local-version-row, .local-version-heading, .vision-save-bar { align-items: stretch; flex-direction: column; }.local-version-actions { justify-content: flex-end; }.vision-toolbar { grid-template-columns: 1fr 1fr; }.vision-toolbar .arco-input-wrapper { grid-column: 1 / -1; } }
.instance-meta-row { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px 24px; }
.instance-meta-row .instance-storage-hint { flex: 1 1 320px; margin: 0; min-width: 0; align-items: center; }
.instance-meta-row .instance-storage-hint code { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.opencodex-page { color: var(--text-main); --oc-border: var(--line); }
.version-manager-card, .version-header-card > div, .service-control-card, .overview-card, .quick-card { background: var(--surface-strong); border-color: var(--oc-border); }
.version-manager-card, .overview-card, .quick-card { box-shadow: 0 10px 24px rgba(30, 53, 84, .04); }
.opencodex-tabs button.active { color: var(--blue); border-bottom-color: var(--blue); }
/* Container breakpoints follow the content area, including the collapsible sidebar. */
@container (max-width: 780px) {
  .opencodex-title-row h1 { font-size: 22px; }
  .instance-target-bar { margin-left: 0; }
  .version-header-card { gap: 10px; }
  .version-header-card > div { padding: 16px; }
  .version-metric-icon { display: none; }
}
@container (max-width: 560px) {
  .opencodex-hero { align-items: flex-start; }
  .opencodex-hero > div:first-child { flex-basis: 100%; }
  .instance-storage-hint { flex-direction: column; gap: 4px; }
  .version-manager-card { padding: 16px; }
  .version-header-card { grid-template-columns: minmax(0, 1fr); }
  .version-header-card > div + div { border: 1px solid #e4e7ec; }
  .latest-release { grid-template-columns: 40px minmax(0, 1fr); }
  .latest-release > .arco-btn { grid-column: 1 / -1; justify-self: end; }
  .local-version-row, .local-version-table-heading { grid-template-columns: minmax(0, 1fr) auto; gap: 8px; }
  .local-version-table-heading > span:nth-child(2) { display: none; }
  .local-version-row .version-state { grid-column: 1; grid-row: 2; }
  .local-version-actions { grid-column: 2; grid-row: 1 / 3; }
  .release-picker { grid-template-columns: minmax(0, 1fr); }
}

/* ===== OpenCodex 视觉系统：与应用主色（蓝色系 / 半透明白卡片）保持一致 ===== */
.opencodex-page {
  --oc-ink: var(--text-main, #101827);
  --oc-muted: var(--text-soft, #66758c);
  --oc-line: rgba(85, 113, 156, .18);
  --oc-surface: rgba(255, 255, 255, .9);
  --oc-soft: #f6f9fe;
  --oc-accent: var(--blue, #2563eb);
  --oc-accent-soft: #eaf1ff;
  --oc-accent-hover: #1d4fd8;
  --oc-success: #12b981;
  color: var(--oc-ink);
}
.opencodex-page .opencodex-hero { padding: 4px 0 2px; }
.opencodex-page .opencodex-brand-mark {
  width: 46px;
  height: 46px;
  border-radius: 12px;
  color: #fff;
  background: linear-gradient(135deg, #2563eb, #0891b2);
  box-shadow: 0 10px 24px rgba(37, 99, 235, .22);
}
.opencodex-page .opencodex-title-row h1 { color: var(--oc-ink); font-size: 23px; }
.opencodex-page .opencodex-title-row p { color: var(--oc-muted); }
.opencodex-page .opencodex-status-strip > span:not(.status-dot) { border-left-color: var(--oc-line); }
.opencodex-page .opencodex-tabs { border-bottom-color: var(--oc-line); }
.opencodex-page .opencodex-tabs button { color: var(--oc-muted); border-radius: 8px 8px 0 0; }
.opencodex-page .opencodex-tabs button.active { color: var(--oc-accent); border-bottom-color: var(--oc-accent); }
.opencodex-page .opencodex-tabs button:hover { color: var(--oc-accent); background: var(--oc-accent-soft); }
.opencodex-page .status-dot.online { background: var(--oc-success); box-shadow: 0 0 0 5px rgba(18, 185, 129, .14); }
.opencodex-page .engine-setup-notice { border-color: var(--oc-line); background: var(--oc-surface); }
.opencodex-page .service-control-card,
.opencodex-page .overview-card,
.opencodex-page .quick-card,
.opencodex-page .console-card,
.opencodex-page .version-manager-card,
.opencodex-page .version-header-card > div,
.opencodex-page .logs-card,
.opencodex-page .web-management-card,
.opencodex-page .vision-manager-card,
.opencodex-page .vision-offline-card {
  border-color: var(--oc-line);
  border-radius: 10px;
  background: var(--oc-surface);
  box-shadow: 0 12px 30px rgba(20, 35, 55, .06);
  backdrop-filter: blur(14px);
}
.opencodex-page .overview-card > strong,
.opencodex-page .section-heading h2 { color: var(--oc-ink); }
.opencodex-page .overview-icon.blue, .opencodex-page .quick-grid button > span { color: var(--oc-accent); background: var(--oc-accent-soft); }
.opencodex-page .overview-icon.green { color: #07866f; background: #e1f5ef; }
.opencodex-page .overview-icon.cyan { color: var(--cyan, #0891b2); background: #e3f7fb; }
.opencodex-page .overview-icon.violet { color: #7c3aed; background: #f1eaff; }
.opencodex-page .quick-grid button { border-color: var(--oc-line); background: var(--oc-soft); }
.opencodex-page .quick-grid button:hover:not(:disabled) { border-color: var(--line-strong, #93b4f8); background: #eef4ff; }
.opencodex-page .health-indicator.healthy { color: #0f9f6e; }
.opencodex-page .console-output { background: #111c2e; color: #cfe9c0; }
.opencodex-page .console-line time { color: #6b7d99; }
/* 头部只放标题与说明；两张设置卡（生图上游 / 描述器）在下方等宽并排。 */
.opencodex-page .vision-manager-header { grid-template-columns: 1fr; padding-bottom: 18px; border-bottom: 0; background: linear-gradient(120deg, #f2f7ff, #f7fbff 60%, #eef9f7); }
.vision-editor-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; padding: 0 24px 20px; border-bottom: 1px solid rgba(85, 113, 156, .13); background: linear-gradient(120deg, #f2f7ff, #f7fbff 60%, #eef9f7); }
.vision-editor-grid > .vision-sidecar-editor { align-content: start; }
@media (max-width: 1100px) { .vision-editor-grid { grid-template-columns: 1fr; } }
.image-gen-editor { grid-template-columns: minmax(0, 1fr) minmax(110px, 140px); }
.image-gen-editor :deep(.arco-input-number) { border: 1px solid var(--oc-line, #d9dee8); border-radius: 6px; background: #fff; }
.image-gen-option { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; min-width: 0; }
.image-gen-option > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.image-gen-option > small { flex: 0 0 auto; color: var(--oc-muted, #6d788a); font-size: 11px; }
.image-gen-recent { display: grid; grid-column: 1 / -1; gap: 4px; padding: 10px 12px; border: 1px dashed var(--oc-line, #d9dee8); border-radius: 8px; background: rgba(247, 249, 252, .9); }
.image-gen-recent > small { color: var(--oc-muted, #6d788a); font-size: 11px; }
.image-gen-recent-row { display: grid; grid-template-columns: 40px minmax(0, 1fr) auto auto; align-items: center; gap: 10px; font-size: 12px; font-variant-numeric: tabular-nums; }
.image-gen-recent-status { display: inline-block; padding: 1px 0; border-radius: 4px; font-weight: 700; text-align: center; }
.image-gen-recent-row.ok .image-gen-recent-status { color: #0f766e; background: rgba(15, 118, 110, .1); }
.image-gen-recent-row.failed .image-gen-recent-status { color: #b42318; background: rgba(180, 35, 24, .1); }
.image-gen-recent-model { overflow: hidden; color: var(--oc-ink, #172235); text-overflow: ellipsis; white-space: nowrap; }
.image-gen-recent-meta { color: #b45309; font-size: 11px; }
.image-gen-recent-row.ok .image-gen-recent-meta { color: var(--oc-muted, #6d788a); }
.image-gen-recent-time { color: var(--oc-muted, #6d788a); font-size: 11px; }
.image-gen-recent > .vision-sidecar-hint { margin-top: 4px; }
.vision-sidecar-hint.warning { color: #b45309; }
.opencodex-page .vision-kicker { color: var(--oc-accent); }
.opencodex-page .vision-offline-icon, .opencodex-page .web-orb { color: var(--oc-accent); background: var(--oc-accent-soft); }
.opencodex-page .vision-model-row:hover:not(.disabled):not(.native), .opencodex-page .vision-model-row.selected { background: #f0f6ff; }
.opencodex-page .vision-save-bar { background: var(--oc-soft); }
.opencodex-page .latest-release { background: linear-gradient(135deg, #eef4ff, #f3fbff); border-color: #d6e3fb; }
.vision-sidecar-editor { display: grid; grid-template-columns: minmax(0, 1fr) minmax(150px, 176px); gap: 10px; padding: 16px; border: 1px solid var(--oc-line, #d9dee8); border-radius: 10px; background: rgba(255, 255, 255, .92); box-shadow: 0 8px 22px rgba(37, 99, 235, .06); }
.vision-sidecar-editor-title { display: flex; grid-column: 1 / -1; align-items: center; justify-content: space-between; gap: 12px; }
.vision-sidecar-editor-title > div { display: grid; gap: 2px; min-width: 0; }
.vision-sidecar-editor-title small, .vision-sidecar-field > span { color: var(--oc-muted, #6d788a); font-size: 11px; }
.vision-sidecar-editor-title strong { overflow: hidden; color: var(--oc-ink, #172235); font-size: 13px; text-overflow: ellipsis; white-space: nowrap; }
.vision-sidecar-field { display: grid; min-width: 0; gap: 5px; align-content: start; }
.vision-sidecar-hint { display: flex; grid-column: 1 / -1; align-items: flex-start; gap: 6px; margin: -2px 0 0; color: var(--oc-muted, #6d788a); font-size: 11px; line-height: 1.5; }
.vision-sidecar-hint > svg { flex: 0 0 auto; margin-top: 2px; }
.vision-sidecar-editor :deep(.arco-select-view) { border: 1px solid var(--oc-line, #d9dee8); border-radius: 6px; background: #fff; }
.vision-sidecar-editor > .arco-btn { grid-column: 1 / -1; justify-self: stretch; }
.vision-sidecar-error { grid-column: 1 / -1; margin: 0; color: #c4233b; font-size: 11px; line-height: 1.45; overflow-wrap: anywhere; }
.opencodex-page :deep(.arco-btn-primary.arco-btn-status-normal:not(.arco-btn-disabled)) { background: var(--oc-accent); border-color: var(--oc-accent); }
.opencodex-page :deep(.arco-btn-primary.arco-btn-status-normal:not(.arco-btn-disabled):hover) { background: var(--oc-accent-hover); border-color: var(--oc-accent-hover); }
.opencodex-page .instance-target-bar :deep(.arco-select-view),
.opencodex-page .release-picker :deep(.arco-select-view) { border-color: var(--oc-line); }
@container (max-width: 780px) {
  .opencodex-page .vision-manager-header { grid-template-columns: minmax(0, 1fr); }
}
@container (max-width: 480px) {
  .vision-sidecar-editor { grid-template-columns: minmax(0, 1fr); }
}
</style>
