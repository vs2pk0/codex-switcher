<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  activateBundledOpenCodexEngine,
  deleteOpenCodexSwitcherAccount,
  deleteOpenCodexEngine,
  getOpenCodexEngineCatalog,
  getOpenCodexSnapshot,
  getOpenCodexVisionModels,
  importOpenCodexSwitcherAccounts,
  installOpenCodexEngine,
  openOpenCodexDashboard,
  readOpenCodexLogs,
  runOpenCodexAction,
  scanOpenCodexSwitcherAccounts,
  subscribeOpenCodexEvents,
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
  OpenCodexPage,
  OpenCodexSettings,
  OpenCodexSwitcherAccountScan,
  OpenCodexSystemSnapshot,
  OpenCodexVisionModelCatalog,
} from "./types";

const props = defineProps<{ active: boolean }>();
const emit = defineEmits<{ (event: "accounts-refreshed"): void }>();

const page = ref<OpenCodexPage>("console");
const snapshot = ref<OpenCodexSystemSnapshot | null>(null);
const logs = ref<OpenCodexCommandLogEvent[]>([]);
const busy = ref(false);
const loading = ref(false);
const interactiveOperationId = ref("");
const commandInput = ref("");
const catalog = ref<OpenCodexEngineCatalog | null>(null);
const catalogLoading = ref(false);
const selectedVersion = ref("");
const accountScan = ref<OpenCodexSwitcherAccountScan | null>(null);
const accountScanLoading = ref(false);
const selectedAccountIds = ref<string[]>([]);
const importingAccounts = ref(false);
const deletingAccountId = ref("");
const visionCatalog = ref<OpenCodexVisionModelCatalog | null>(null);
const visionLoading = ref(false);
const visionSaving = ref(false);
const visionSearch = ref("");
const selectedVisionModels = ref<string[]>([]);
let unlistenEvents: UnlistenFn | undefined;
const answeredPortPrompts = new Set<string>();

function loadSettings(): OpenCodexSettings {
  try {
    const stored = localStorage.getItem("codex-switcher-opencodex-settings");
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

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function appendLog(event: OpenCodexCommandLogEvent): void {
  logs.value = [...logs.value.slice(-999), event];
  if (isOpenCodexPortPrompt(event.line) && !answeredPortPrompts.has(event.operationId)) {
    answeredPortPrompts.add(event.operationId);
    const selectedPort = settings.value.port || DEFAULT_OPEN_CODEX_PORT;
    void writeOpenCodexInput(event.operationId, `${selectedPort}\n`)
      .then(() => {
        logs.value = [...logs.value.slice(-999), {
          operationId: event.operationId,
          stream: "system",
          line: `已使用管理器默认端口 ${selectedPort}`,
          timestamp: new Date().toISOString(),
        }];
      })
      .catch((error) => Message.error(`自动填写 OpenCodex 端口失败：${errorText(error)}`));
  }
}

async function refreshSnapshot(showError = true): Promise<void> {
  loading.value = true;
  try {
    snapshot.value = await getOpenCodexSnapshot();
    if (snapshot.value.running && snapshot.value.port) settings.value.port = snapshot.value.port;
  } catch (error) {
    if (showError) Message.error(`读取 OpenCodex 状态失败：${errorText(error)}`);
  } finally {
    loading.value = false;
  }
}

async function run(action: OpenCodexAction): Promise<void> {
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
            title: "恢复原生 Codex",
            content: "将停止 OpenCodex 服务并恢复原生 Codex 配置，是否继续？",
          }
        : action === "uninstall"
          ? {
              title: "卸载 OpenCodex",
              content: "将执行 OpenCodex 官方卸载流程。配置备份仍由 OpenCodex 自身策略处理，是否继续？",
            }
          : action === "service_install"
            ? {
                title: "开启后台服务",
                content: "将注册并启动 OpenCodex 系统后台服务，使其在登录后自动运行。是否继续？",
              }
            : {
                title: "取消后台服务",
                content: "将停止并移除 OpenCodex 后台服务，同时恢复原生 Codex 配置；账号和 OpenCodex 配置仍会保留。是否继续？",
              };
      Modal.warning({
        title: confirmation.title,
        content: confirmation.content,
        okText: "确认执行",
        cancelText: "取消",
        hideCancel: false,
        onOk: () => resolve(true),
        onCancel: () => resolve(false),
        onClose: () => resolve(false),
      });
    });
    if (!confirmed) return;
  }
  busy.value = true;
  try {
    const started = await runOpenCodexAction(action, settings.value.port);
    interactiveOperationId.value = started.interactive ? started.operationId : "";
    if (started.interactive) page.value = "console";
  } catch (error) {
    busy.value = false;
    Message.error(`OpenCodex 操作启动失败：${errorText(error)}`);
  }
}

async function submitCommandInput(): Promise<void> {
  const operationId = interactiveOperationId.value;
  if (!operationId) return;
  try {
    await writeOpenCodexInput(operationId, `${commandInput.value}\n`);
    commandInput.value = "";
  } catch (error) {
    Message.error(`发送初始化输入失败：${errorText(error)}`);
  }
}

async function openDashboard(mode = settings.value.dashboardOpenMode): Promise<void> {
  try {
    await openOpenCodexDashboard(mode, effectivePort.value);
  } catch (error) {
    Message.error(`打开 OpenCodex Dashboard 失败：${errorText(error)}`);
  }
}

async function checkVersions(): Promise<void> {
  if (catalogLoading.value) return;
  catalogLoading.value = true;
  try {
    catalog.value = await getOpenCodexEngineCatalog();
    selectedVersion.value =
      selectedVersion.value ||
      catalog.value.latestStable?.version ||
      catalog.value.releases[0]?.version ||
      "";
    if (catalog.value.remoteError) {
      Message.warning(`GitHub 版本暂时不可用，本地 Engine 仍可管理：${catalog.value.remoteError}`);
    }
  } catch (error) {
    Message.error(`检测 OpenCodex Engine 版本失败：${errorText(error)}`);
  } finally {
    catalogLoading.value = false;
  }
}

async function applyRelease(release: OpenCodexEngineRelease): Promise<void> {
  if (snapshot.value?.running) {
    Message.warning("请先停止 OpenCodex 服务，再更新或切换 Engine");
    return;
  }
  busy.value = true;
  try {
    const result = await installOpenCodexEngine(release.version);
    Message.success(result.message);
    await Promise.all([refreshSnapshot(false), checkVersions()]);
  } catch (error) {
    Message.error(`安装 Engine 失败：${errorText(error)}`);
  } finally {
    busy.value = false;
  }
}

async function rollbackEngine(): Promise<void> {
  if (snapshot.value?.running) {
    Message.warning("请先停止 OpenCodex 服务，再回退 Engine");
    return;
  }
  busy.value = true;
  try {
    const result = await activateBundledOpenCodexEngine();
    Message.success(result.message);
    await Promise.all([refreshSnapshot(false), checkVersions()]);
  } catch (error) {
    Message.error(`回退 Engine 失败：${errorText(error)}`);
  } finally {
    busy.value = false;
  }
}

async function switchInstalledVersion(version: string): Promise<void> {
  if (snapshot.value?.running) {
    Message.warning("请先停止 OpenCodex 服务，再切换 Engine");
    return;
  }
  busy.value = true;
  try {
    const result = await installOpenCodexEngine(version);
    Message.success(result.message);
    await Promise.all([refreshSnapshot(false), checkVersions()]);
  } catch (error) {
    Message.error(`切换 Engine 失败：${errorText(error)}`);
  } finally {
    busy.value = false;
  }
}

function confirmDeleteInstalledVersion(version: string): void {
  Modal.warning({
    title: "删除本地 Engine",
    content: `确认删除本地 Engine v${version}？删除后仍可从 GitHub 版本列表重新下载安装。`,
    okText: "删除",
    cancelText: "取消",
    hideCancel: false,
    async onOk() {
      busy.value = true;
      try {
        const result = await deleteOpenCodexEngine(version);
        Message.success(result.message);
        await checkVersions();
      } catch (error) {
        Message.error(`删除 Engine 失败：${errorText(error)}`);
      } finally {
        busy.value = false;
      }
    },
  });
}

function saveSettings(): void {
  if (!Number.isInteger(settings.value.port) || settings.value.port < 1024 || settings.value.port > 65535) {
    Message.warning("端口必须在 1024–65535 之间");
    return;
  }
  localStorage.setItem(
    "codex-switcher-opencodex-settings",
    serializeOpenCodexSettings(settings.value),
  );
  Message.success("OpenCodex 设置已保存，下一次启动服务时生效");
}

async function scanAccounts(): Promise<void> {
  accountScanLoading.value = true;
  try {
    accountScan.value = await scanOpenCodexSwitcherAccounts();
    selectedAccountIds.value = accountScan.value.accounts
      .filter((account) => account.eligible)
      .map((account) => account.sourceId);
  } catch (error) {
    Message.error(`扫描 Switcher 账号失败：${errorText(error)}`);
  } finally {
    accountScanLoading.value = false;
  }
}

async function loadVisionModels(): Promise<void> {
  if (!snapshot.value?.running) {
    visionCatalog.value = null;
    selectedVisionModels.value = [];
    return;
  }
  visionLoading.value = true;
  try {
    visionCatalog.value = await getOpenCodexVisionModels();
    selectedVisionModels.value = visionCatalog.value.models
      .filter((model) => model.sidecarEnabled)
      .map((model) => model.namespaced);
  } catch (error) {
    Message.error(`读取 OpenCodex 模型失败：${errorText(error)}`);
  } finally {
    visionLoading.value = false;
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
    const result = await updateOpenCodexVisionModels(models);
    Message.success(result.message);
    await Promise.all([refreshSnapshot(false), loadVisionModels()]);
    if (result.changedProviders.length) {
      Modal.info({
        title: "图片模型已同步",
        content: "Codex 客户端会缓存模型能力。请用 Command + Q（Windows 使用退出菜单）完全退出 Codex，再重新打开后使用图片输入。",
        okText: "知道了",
      });
    }
  } catch (error) {
    Message.error(`保存图片模型失败：${errorText(error)}`);
  } finally {
    visionSaving.value = false;
  }
}

async function importAccounts(): Promise<void> {
  if (!selectedImportAccountIds.value.length) {
    Message.warning("请至少选择一个可导入账号");
    return;
  }
  importingAccounts.value = true;
  try {
    const result = await importOpenCodexSwitcherAccounts(selectedImportAccountIds.value);
    Message.success(`已导入 ${result.importedCount} 个账号，跳过 ${result.skippedCount} 个`);
    emit("accounts-refreshed");
    await scanAccounts();
  } catch (error) {
    Message.error(`导入 OpenCodex 账号失败：${errorText(error)}`);
  } finally {
    importingAccounts.value = false;
  }
}

function confirmDeleteSelectedAccounts(): void {
  const selected = [...selectedDeleteAccounts.value];
  if (!selected.length || deletingAccountId.value) {
    Message.warning("请至少选择一个已导入且可删除的账号");
    return;
  }
  if (snapshot.value?.running) {
    Message.warning("请先停止 OpenCodex 服务，再删除账号");
    return;
  }
  Modal.warning({
    title: "批量删除 OpenCodex 账号",
    content: `确认从 OpenCodex 删除所选 ${selected.length} 个账号？Switcher 账号总览中的原账号会保留。`,
    okText: "删除",
    cancelText: "取消",
    hideCancel: false,
    onOk: async () => {
      deletingAccountId.value = "__selected__";
      try {
        const results = [];
        for (const account of selected) {
          results.push(await deleteOpenCodexSwitcherAccount(account.sourceId));
        }
        const deletedCount = results.filter((result) => result.deleted).length;
        const failures = results.filter((result) => !result.deleted && /身份|不匹配|无法确认/.test(result.message));
        if (failures.length) {
          Message.warning(`已删除 ${deletedCount} 个账号，${failures.length} 个账号因身份校验失败而保留`);
        } else {
          Message.success(`已从 OpenCodex 删除 ${deletedCount} 个账号`);
        }
        emit("accounts-refreshed");
        await scanAccounts();
      } catch (error) {
        Message.error(`批量删除 OpenCodex 账号失败：${errorText(error)}`);
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
  if (!account.deletable || deletingAccountId.value) return;
  if (snapshot.value?.running) {
    Message.warning("请先停止 OpenCodex 服务，再删除账号");
    return;
  }
  Modal.warning({
    title: "删除 OpenCodex 账号",
    content: `确认从 OpenCodex 删除 ${account.email || account.sourceId}？Switcher 账号总览中的原账号会保留。`,
    okText: "删除",
    cancelText: "取消",
    hideCancel: false,
    onOk: async () => {
      deletingAccountId.value = account.sourceId;
      try {
        const result = await deleteOpenCodexSwitcherAccount(account.sourceId);
        Message.success(result.message);
        emit("accounts-refreshed");
        await scanAccounts();
      } catch (error) {
        Message.error(`删除 OpenCodex 账号失败：${errorText(error)}`);
        throw error;
      } finally {
        deletingAccountId.value = "";
      }
    },
  });
}

function formatLogTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleTimeString("zh-CN", { hour12: false });
}

function accountInitial(email: string): string {
  const initial = email.replace(/[^a-zA-Z0-9\u4e00-\u9fff]/g, "").charAt(0);
  return initial ? initial.toUpperCase() : "C";
}

function shortSourceId(sourceId: string): string {
  return sourceId.length > 16 ? `${sourceId.slice(0, 8)}…${sourceId.slice(-5)}` : sourceId;
}

function accountStatusLabel(status: string): string {
  if (status === "ready") return "待导入";
  if (status === "already_imported") return "已导入";
  if (status === "unsupported") return "不支持";
  return "无效";
}

function toggleMigrationAccount(account: OpenCodexSwitcherAccountScan["accounts"][number]): void {
  if (!account.eligible && !account.deletable) return;
  selectedAccountIds.value = selectedAccountIds.value.includes(account.sourceId)
    ? selectedAccountIds.value.filter((id) => id !== account.sourceId)
    : [...selectedAccountIds.value, account.sourceId];
}

watch(
  () => props.active,
  (active) => {
    if (active) void refreshSnapshot(false);
  },
);

watch(page, (nextPage) => {
  if (nextPage === "versions" && !catalog.value) void checkVersions();
  if (nextPage === "vision") void loadVisionModels();
});

onMounted(async () => {
  try {
    const persisted = await readOpenCodexLogs(300);
    logs.value = persisted.map((line, index) => ({
      operationId: "history",
      stream: line.includes("stderr") ? "stderr" : "system",
      line,
      timestamp: new Date(Date.now() - (persisted.length - index) * 10).toISOString(),
    }));
    unlistenEvents = await subscribeOpenCodexEvents(
      appendLog,
      (event) => {
        busy.value = false;
        if (interactiveOperationId.value === event.operationId) interactiveOperationId.value = "";
        appendLog({
          operationId: event.operationId,
          stream: event.success ? "system" : "stderr",
          line: event.message,
          timestamp: event.timestamp,
        });
        if (event.success) Message.success(event.message);
        else Message.error(event.message);
        void refreshSnapshot(false);
      },
    );
    await refreshSnapshot(false);
  } catch (error) {
    Message.error(`初始化 OpenCodex 管理模块失败：${errorText(error)}`);
  }
});

onUnmounted(() => unlistenEvents?.());
</script>

<template>
  <section class="opencodex-page">
    <header class="opencodex-hero">
      <div>
        <div class="opencodex-title-row">
          <span class="opencodex-brand-mark">OC</span>
          <div>
            <h1>OpenCodex Manager</h1>
            <p>本地代理、Codex 集成与 Engine 版本控制台</p>
          </div>
        </div>
      </div>
      <div class="opencodex-status-strip">
        <span :class="['status-dot', snapshot?.running ? 'online' : 'offline']" />
        <strong>{{ snapshot?.running ? "服务运行中" : "服务未运行" }}</strong>
        <span>Engine {{ snapshot?.engineVersion ? `v${snapshot.engineVersion}` : "不可用" }}</span>
        <span>{{ snapshot?.platform || "检测中" }}</span>
        <span>端口 {{ effectivePort }}</span>
      </div>
    </header>

    <nav class="opencodex-tabs" aria-label="OpenCodex 功能导航">
      <button :class="{ active: page === 'console' }" @click="page = 'console'">
        <icon-command />运行控制台
      </button>
      <button :class="{ active: page === 'web' }" @click="page = 'web'">
        <icon-public />Web 管理
      </button>
      <button :class="{ active: page === 'vision' }" @click="page = 'vision'">
        <icon-image />图片模型
      </button>
      <button :class="{ active: page === 'versions' }" @click="page = 'versions'">
        <icon-apps />版本管理
      </button>
      <button :class="{ active: page === 'logs' }" @click="page = 'logs'">
        <icon-file />运行日志
      </button>
      <button :class="{ active: page === 'settings' }" @click="page = 'settings'">
        <icon-settings />设置
      </button>
    </nav>

    <a-spin :loading="loading" class="opencodex-content" tip="正在读取 OpenCodex 状态…">
      <template v-if="page === 'console'">
        <section class="service-control-card">
          <div class="service-actions">
            <a-button type="primary" :disabled="busy || snapshot?.running || !snapshot?.initialized" @click="run('start')">
              <template #icon><icon-play-arrow /></template>启动服务
            </a-button>
            <a-button :disabled="busy || !snapshot?.running" @click="run('stop')">
              <template #icon><icon-pause /></template>停止
            </a-button>
            <a-button :disabled="busy || !snapshot?.initialized" @click="run('restart')">
              <template #icon><icon-refresh /></template>重启
            </a-button>
            <a-button :disabled="busy" @click="run('init')">
              <template #icon><icon-command /></template>{{ snapshot?.initialized ? "重新初始化" : "初始化" }}
            </a-button>
            <a-button type="text" :disabled="loading" @click="refreshSnapshot()">
              <template #icon><icon-sync /></template>刷新状态
            </a-button>
          </div>
          <div class="health-indicator">
            <icon-heart-fill />
            <span><small>服务健康</small><strong>{{ snapshot?.ready ? "正常" : snapshot?.running ? "正在就绪" : "未运行" }}</strong></span>
          </div>
        </section>

        <section class="overview-grid">
          <article class="overview-card">
            <span class="overview-icon green"><icon-apps /></span>
            <small>Engine 版本</small>
            <strong>{{ snapshot?.engineVersion ? `v${snapshot.engineVersion}` : "资源不可用" }}</strong>
            <p>{{ snapshot?.engineSource === "managed" ? "在线安装版本" : "客户端内置基线" }}</p>
          </article>
          <article class="overview-card">
            <span class="overview-icon blue"><icon-thunderbolt /></span>
            <small>服务状态</small>
            <strong>{{ snapshot?.running ? "运行中" : "已停止" }}</strong>
            <p>{{ snapshot?.pid ? `PID ${snapshot.pid}` : "暂无运行进程" }}</p>
          </article>
          <article class="overview-card access-card">
            <span class="overview-icon cyan"><icon-public /></span>
            <small>访问地址</small>
            <strong>localhost:{{ effectivePort }}</strong>
            <p>
              <a-link :disabled="!snapshot?.running" @click="openDashboard('client')">客户端打开</a-link>
              <a-link :disabled="!snapshot?.running" @click="openDashboard('browser')">浏览器打开</a-link>
            </p>
          </article>
          <article class="overview-card">
            <span class="overview-icon violet"><icon-link /></span>
            <small>Codex 集成</small>
            <strong>{{ snapshot?.integrationStatus || "等待检测" }}</strong>
            <p>{{ snapshot?.initialized ? "配置与模型可同步" : "需要完成首次初始化" }}</p>
          </article>
        </section>

        <section class="quick-card">
          <div class="section-heading"><div><h2>快捷操作</h2><p>所有命令均经过 Rust 白名单和参数校验</p></div></div>
          <div class="quick-grid">
            <button :disabled="busy || !snapshot?.initialized" @click="run('doctor')"><span><icon-bug /></span><div><strong>环境诊断</strong><small>检查运行环境与配置</small></div><icon-right /></button>
            <button :disabled="busy || !snapshot?.initialized" @click="run('sync')"><span><icon-sync /></span><div><strong>同步配置</strong><small>重新同步 Codex 模型</small></div><icon-right /></button>
            <button
              :class="{ 'toggle-enabled': snapshot?.backgroundService?.installed }"
              :disabled="busy || !snapshot?.initialized || snapshot?.backgroundService?.supported === false"
              :title="snapshot?.backgroundService?.summary"
              @click="run(snapshot?.backgroundService?.installed ? 'service_uninstall' : 'service_install')"
            >
              <span><icon-storage /></span>
              <div>
                <strong>{{ snapshot?.backgroundService?.installed ? "取消后台服务" : "后台服务" }}</strong>
                <small>{{ snapshot?.backgroundService?.installed ? "已注册，再次点击取消" : "注册系统启动服务" }}</small>
              </div>
              <em :class="['quick-toggle-state', snapshot?.backgroundService?.installed ? 'on' : 'off']">
                {{ snapshot?.backgroundService?.installed ? "已开启" : "未开启" }}
              </em>
            </button>
            <button :disabled="busy || !snapshot?.initialized" @click="run('restore')"><span><icon-undo /></span><div><strong>恢复 Codex</strong><small>停止服务并还原配置</small></div><icon-right /></button>
          </div>
        </section>

        <section class="console-card">
          <div class="section-heading">
            <div><h2>最近输出</h2><p>凭据和 Token 会在进入前端前自动脱敏</p></div>
            <a-button size="small" @click="page = 'logs'"><template #icon><icon-file /></template>完整日志</a-button>
          </div>
          <div class="console-output">
            <div v-if="!recentLogs.length" class="console-empty">等待 OpenCodex 输出…</div>
            <div v-for="(entry, index) in recentLogs" :key="`${entry.operationId}-${index}`" :class="['console-line', entry.stream]">
              <time>{{ formatLogTime(entry.timestamp) }}</time><span>{{ entry.line }}</span>
            </div>
          </div>
          <form v-if="interactiveOperationId" class="interactive-input" @submit.prevent="submitCommandInput">
            <a-input v-model="commandInput" autofocus placeholder="输入初始化向导答案；直接回车表示使用默认值" />
            <a-button html-type="submit" type="primary"><template #icon><icon-send /></template>发送</a-button>
          </form>
        </section>
      </template>

      <template v-else-if="page === 'web'">
        <section class="web-management-card">
          <div class="web-orb"><icon-public /></div>
          <a-tag :color="snapshot?.running ? 'green' : 'gray'">{{ snapshot?.running ? "服务已连接" : "服务未运行" }}</a-tag>
          <h2>OpenCodex Web Dashboard</h2>
          <p>{{ snapshot?.dashboardUrl || `http://127.0.0.1:${effectivePort}` }}</p>
          <div class="web-actions">
            <a-button type="primary" size="large" :disabled="!snapshot?.running" @click="openDashboard('client')"><template #icon><icon-desktop /></template>客户端窗口打开</a-button>
            <a-button size="large" :disabled="!snapshot?.running" @click="openDashboard('browser')"><template #icon><icon-launch /></template>系统浏览器打开</a-button>
          </div>
          <a-alert v-if="!snapshot?.running" type="warning" show-icon>请先在“运行控制台”启动服务，再打开 Web 管理页面。</a-alert>
        </section>
      </template>

      <template v-else-if="page === 'versions'">
        <section class="version-header-card">
          <div><small>当前 Engine</small><strong>{{ snapshot?.engineVersion ? `v${snapshot.engineVersion}` : "不可用" }}</strong><span>{{ snapshot?.engineSource === "managed" ? "在线版本" : "内置基线" }}</span></div>
          <div><small>最新稳定版</small><strong>{{ catalog?.latestStable ? `v${catalog.latestStable.version}` : "等待检测" }}</strong><span>GitHub Release</span></div>
          <div><small>桌面客户端</small><strong>v{{ snapshot?.desktopVersion || "-" }}</strong><span>{{ snapshot?.platform }}</span></div>
        </section>
        <section class="version-manager-card">
          <div class="section-heading">
            <div><h2>Engine 版本管理</h2><p>使用内置 Bun 下载并校验官方 @bitkyc08/opencodex 包</p></div>
            <a-button type="primary" :loading="catalogLoading" @click="checkVersions"><template #icon><icon-refresh /></template>检测更新</a-button>
          </div>
          <div v-if="catalog?.latestStable" class="latest-release">
            <span class="release-icon"><icon-download /></span>
            <div><a-tag :color="catalog.latestStable.newerThanCurrent ? 'orange' : 'green'">{{ catalog.latestStable.newerThanCurrent ? "发现新版本" : "已是最新稳定版" }}</a-tag><h3>OpenCodex Engine v{{ catalog.latestStable.version }}</h3><p>内置版本始终保留，可随时安全回退。</p></div>
            <a-button type="primary" :disabled="busy || snapshot?.running || !catalog.latestStable.newerThanCurrent" @click="applyRelease(catalog.latestStable)">更新到最新版</a-button>
          </div>
          <div class="local-version-section">
            <div class="local-version-heading">
              <div><h3>本地版本</h3><p>保留当前版本和最近 3 个历史版本，可随时切换或删除非当前版本。</p></div>
              <a-tag color="blue">{{ catalog?.installedVersions.length || 0 }} 个本地版本</a-tag>
            </div>
            <div v-if="catalog?.installedVersions.length" class="local-version-list">
              <div v-for="version in catalog.installedVersions" :key="version" class="local-version-row">
                <div><strong>Engine v{{ version }}</strong><span>{{ catalog.currentVersion === version ? "当前使用" : "本地已安装" }}</span></div>
                <div class="local-version-actions">
                  <a-button size="small" :disabled="busy || snapshot?.running || catalog.currentVersion === version" @click="switchInstalledVersion(version)">切换</a-button>
                  <a-button size="small" status="danger" :disabled="busy || snapshot?.running || catalog.currentVersion === version" @click="confirmDeleteInstalledVersion(version)"><template #icon><icon-delete /></template>删除</a-button>
                </div>
              </div>
            </div>
            <a-empty v-else description="暂无本地历史版本" />
          </div>
          <div class="github-version-section">
            <div class="local-version-heading"><div><h3>GitHub 版本</h3><p>选择官方 Release；未安装时下载并使用，已安装时直接切换。</p></div></div>
          <div class="release-picker">
            <a-select v-model="selectedVersion" placeholder="选择 Engine 版本" :loading="catalogLoading">
              <a-option v-for="release in catalog?.releases || []" :key="release.version" :value="release.version">v{{ release.version }}{{ release.prerelease ? " · 预览" : " · 稳定" }}{{ release.active ? " · 当前" : release.installed ? " · 已安装" : "" }}</a-option>
            </a-select>
            <a-button :disabled="busy || !selectedRelease || selectedRelease.active || snapshot?.running" @click="selectedRelease && applyRelease(selectedRelease)">{{ selectedRelease?.installed ? "切换版本" : "下载并使用" }}</a-button>
            <a-button v-if="snapshot?.engineSource === 'managed'" :disabled="busy || snapshot?.running" @click="rollbackEngine"><template #icon><icon-undo /></template>回退内置版本</a-button>
          </div>
          </div>
        </section>
      </template>

      <template v-else-if="page === 'vision'">
        <section v-if="!snapshot?.running" class="vision-offline-card">
          <span class="vision-offline-icon"><icon-image /></span>
          <h2>启动服务后配置图片模型</h2>
          <p>模型列表来自当前运行中的 OpenCodex。启动后可选择需要通过图片描述器兼容的模型。</p>
          <a-button type="primary" :disabled="busy || !snapshot?.initialized" @click="run('start')"><template #icon><icon-play-arrow /></template>启动 OpenCodex</a-button>
        </section>
        <section v-else class="vision-manager-card">
          <div class="vision-manager-header">
            <div>
              <span class="vision-kicker">VISION SIDECAR</span>
              <h2>图片输入兼容</h2>
              <p>为本身不识图的模型开启图片粘贴。OpenCodex 会先用图片模型生成描述，再交给所选模型处理。</p>
            </div>
            <div class="vision-sidecar-summary">
              <small>图片描述模型</small>
              <strong>{{ visionCatalog?.sidecarModel || 'OpenCodex 默认模型' }}</strong>
              <span>{{ visionCatalog?.sidecarBackend || '自动选择后端' }}</span>
            </div>
          </div>
          <div class="vision-toolbar">
            <a-input v-model="visionSearch" allow-clear placeholder="搜索 Provider 或模型名称"><template #prefix><icon-search /></template></a-input>
            <a-button :loading="visionLoading" @click="loadVisionModels"><template #icon><icon-refresh /></template>读取当前模型</a-button>
            <a-button :disabled="!sidecarSelectableModels.length" @click="selectFilteredVisionModels">选择当前结果</a-button>
            <a-button :disabled="!selectedVisionCount" @click="clearVisionModels">清空选择</a-button>
          </div>
          <a-spin :loading="visionLoading" tip="正在读取 OpenCodex 当前模型…">
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
                <span v-if="model.nativeVision" class="vision-mode-pill native">原生支持图片</span>
                <span v-else-if="model.disabled" class="vision-mode-pill disabled">模型已禁用</span>
                <span v-else :class="['vision-mode-pill', selectedVisionModels.includes(model.namespaced) ? 'sidecar' : 'text']">{{ selectedVisionModels.includes(model.namespaced) ? '图片转文字' : '仅文本' }}</span>
              </article>
            </div>
            <a-empty v-else description="没有匹配的模型" />
          </a-spin>
          <footer class="vision-save-bar">
            <div><strong>已选择 {{ selectedVisionCount }} 个兼容模型</strong><span>保存会自动重启 OpenCodex 并同步 Codex 模型目录</span></div>
            <a-button type="primary" size="large" :loading="visionSaving" :disabled="visionLoading" @click="saveVisionModels"><template #icon><icon-save /></template>保存并同步</a-button>
          </footer>
        </section>
      </template>

      <template v-else-if="page === 'logs'">
        <section class="logs-card">
          <div class="section-heading">
            <div><h2>运行日志</h2><p>最多保留并展示最近 1000 条脱敏输出</p></div>
            <div><a-button size="small" @click="refreshSnapshot()"><template #icon><icon-refresh /></template>刷新状态</a-button><a-button size="small" @click="logs = []"><template #icon><icon-delete /></template>清空显示</a-button></div>
          </div>
          <div class="console-output full-log">
            <div v-if="!logs.length" class="console-empty">暂无日志</div>
            <div v-for="(entry, index) in logs" :key="`${entry.operationId}-${index}`" :class="['console-line', entry.stream]"><time>{{ formatLogTime(entry.timestamp) }}</time><span>{{ entry.line }}</span></div>
          </div>
          <form v-if="interactiveOperationId" class="interactive-input" @submit.prevent="submitCommandInput"><a-input v-model="commandInput" placeholder="输入初始化向导答案" /><a-button html-type="submit" type="primary">发送</a-button></form>
        </section>
      </template>

      <template v-else>
        <section class="settings-grid">
          <article class="settings-card">
            <div class="section-heading"><div><h2>服务设置</h2><p>管理监听端口和 Dashboard 打开方式</p></div></div>
            <a-form :model="settings" layout="vertical">
              <a-form-item label="服务端口"><a-input-number v-model="settings.port" :min="1024" :max="65535" /></a-form-item>
              <a-form-item label="默认打开方式"><a-radio-group v-model="settings.dashboardOpenMode" type="button"><a-radio value="client">客户端窗口</a-radio><a-radio value="browser">系统浏览器</a-radio></a-radio-group></a-form-item>
              <a-button type="primary" @click="saveSettings"><template #icon><icon-save /></template>保存设置</a-button>
            </a-form>
          </article>
          <article class="settings-card">
            <div class="section-heading"><div><h2>Switcher 账号迁移</h2><p>读取当前 Switcher 账号并转换为 OpenCodex OAuth 账号</p></div><a-button size="small" :loading="accountScanLoading" @click="scanAccounts"><template #icon><icon-search /></template>扫描</a-button></div>
            <div v-if="accountScan" class="scan-summary">
              <span>发现 <strong>{{ accountScan.totalCount }}</strong> 个账号</span>
              <span class="scan-ready">可导入 <strong>{{ accountScan.eligibleCount }}</strong> 个</span>
              <span>已选择 <strong>{{ selectedAccountIds.length }}</strong> 个</span>
            </div>
            <div v-if="accountScan?.accounts.length" class="migration-list">
              <article
                v-for="account in accountScan.accounts"
                :key="account.sourceId"
                class="migration-account-card"
                :class="{
                  selected: selectedAccountIds.includes(account.sourceId),
                  current: account.current,
                  unavailable: !account.eligible && !account.deletable,
                }"
                :role="account.eligible || account.deletable ? 'checkbox' : undefined"
                :aria-checked="account.eligible || account.deletable ? selectedAccountIds.includes(account.sourceId) : undefined"
                :aria-disabled="!account.eligible && !account.deletable"
                :tabindex="account.eligible || account.deletable ? 0 : -1"
                @click="toggleMigrationAccount(account)"
                @keydown.space.prevent="toggleMigrationAccount(account)"
                @keydown.enter.prevent="toggleMigrationAccount(account)"
              >
                <span class="migration-check" @click.stop>
                  <a-checkbox
                    :model-value="selectedAccountIds.includes(account.sourceId)"
                    :disabled="!account.eligible && !account.deletable"
                    @change="toggleMigrationAccount(account)"
                  />
                </span>
                <span class="migration-avatar">{{ accountInitial(account.email) }}</span>
                <span class="migration-identity">
                  <span class="migration-account-title">
                    <strong :title="account.email || account.sourceId">{{ account.email || account.sourceId }}</strong>
                    <span v-if="account.current" class="migration-current-pill">当前</span>
                    <span v-if="account.plan" class="migration-plan-pill">{{ account.plan }}</span>
                  </span>
                  <small>账号 ID：{{ shortSourceId(account.sourceId) }}</small>
                  <span class="migration-reason">{{ account.reason }}</span>
                </span>
                <span class="migration-card-actions">
                  <span :class="['migration-status-pill', account.status]">
                    {{ accountStatusLabel(account.status) }}
                  </span>
                  <a-tooltip v-if="account.deletable" content="从 OpenCodex 删除，保留 Switcher 原账号">
                    <a-button
                      class="migration-delete-button"
                      size="mini"
                      status="danger"
                      :loading="deletingAccountId === account.sourceId"
                      :disabled="Boolean(deletingAccountId) || snapshot?.running"
                      aria-label="删除 OpenCodex 账号"
                      @click.stop="confirmDeleteMigratedAccount(account)"
                      @keydown.stop
                    >
                      <template #icon><icon-delete /></template>
                      删除
                    </a-button>
                  </a-tooltip>
                </span>
              </article>
            </div>
            <div v-if="accountScan" class="migration-batch-actions">
              <a-button type="primary" :loading="importingAccounts" :disabled="!selectedImportAccountIds.length || snapshot?.running" @click="importAccounts"><template #icon><icon-import /></template>导入所选（{{ selectedImportAccountIds.length }}）</a-button>
              <a-button status="danger" :loading="deletingAccountId === '__selected__'" :disabled="!selectedDeleteAccounts.length || Boolean(deletingAccountId) || snapshot?.running" @click="confirmDeleteSelectedAccounts"><template #icon><icon-delete /></template>删除所选（{{ selectedDeleteAccounts.length }}）</a-button>
            </div>
            <a-alert v-if="snapshot?.running" type="warning" show-icon>导入前请先停止 OpenCodex 服务，避免配置被运行中的 Engine 覆盖。</a-alert>
          </article>
          <article class="settings-card danger-card">
            <div class="section-heading"><div><h2>维护与恢复</h2><p>执行前请确认当前没有正在处理的请求</p></div></div>
            <div class="danger-actions"><a-button status="warning" :disabled="busy || !snapshot?.initialized" @click="run('restore')"><template #icon><icon-undo /></template>恢复原生 Codex</a-button><a-button status="danger" :disabled="busy" @click="run('uninstall')"><template #icon><icon-delete /></template>卸载 OpenCodex</a-button></div>
          </article>
        </section>
      </template>
    </a-spin>
  </section>
</template>

<style scoped>
.opencodex-page { display: grid; gap: 18px; color: #101827; }
.opencodex-hero { display: flex; align-items: center; justify-content: space-between; gap: 20px; }
.opencodex-title-row { display: flex; align-items: center; gap: 14px; }
.opencodex-brand-mark { display: grid; width: 52px; height: 52px; place-items: center; border-radius: 16px; color: #fff; background: linear-gradient(145deg, #0f766e, #16a085); box-shadow: 0 14px 32px rgba(15, 118, 110, .22); font-size: 17px; font-weight: 900; }
.opencodex-title-row h1 { margin: 0; font-size: clamp(28px, 3vw, 42px); letter-spacing: -.04em; }
.opencodex-title-row p { margin: 5px 0 0; color: #66758c; font-size: 15px; }
.opencodex-status-strip { display: flex; min-height: 54px; align-items: center; gap: 16px; padding: 0 20px; border: 1px solid rgba(85, 113, 156, .18); border-radius: 14px; background: rgba(255, 255, 255, .78); box-shadow: 0 12px 30px rgba(31, 55, 88, .06); color: #65748a; white-space: nowrap; }
.opencodex-status-strip > span:not(.status-dot) { padding-left: 16px; border-left: 1px solid rgba(85, 113, 156, .14); }
.status-dot { width: 10px; height: 10px; border-radius: 50%; }.status-dot.online { background: #16a34a; box-shadow: 0 0 0 5px rgba(22, 163, 74, .12); }.status-dot.offline { background: #94a3b8; }
.opencodex-tabs { display: flex; gap: 6px; padding: 6px; border: 1px solid rgba(85, 113, 156, .16); border-radius: 14px; background: rgba(255, 255, 255, .62); }
.opencodex-tabs button { display: flex; min-height: 40px; align-items: center; gap: 8px; padding: 0 16px; border: 0; border-radius: 9px; color: #596981; background: transparent; font-weight: 760; cursor: pointer; }.opencodex-tabs button.active { color: #0f766e; background: #e8f7f4; box-shadow: 0 6px 16px rgba(15, 118, 110, .09); }
.opencodex-content { display: block; min-height: 460px; }
.service-control-card, .quick-card, .console-card, .version-manager-card, .logs-card, .settings-card, .web-management-card { border: 1px solid rgba(85, 113, 156, .17); border-radius: 16px; background: rgba(255, 255, 255, .82); box-shadow: 0 14px 34px rgba(30, 53, 84, .06); }
.service-control-card { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 16px 18px; }.service-actions { display: flex; flex-wrap: wrap; gap: 10px; }.health-indicator { display: flex; align-items: center; gap: 10px; color: #0f766e; }.health-indicator span { display: grid; }.health-indicator small { color: #718096; }.health-indicator strong { font-size: 15px; }
.overview-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 14px; margin-top: 16px; }.overview-card { position: relative; min-width: 0; padding: 20px; border: 1px solid rgba(85, 113, 156, .17); border-radius: 15px; background: rgba(255, 255, 255, .8); box-shadow: 0 12px 28px rgba(30, 53, 84, .05); }.overview-card > small { display: block; color: #66758c; font-weight: 700; }.overview-card > strong { display: block; overflow: hidden; margin-top: 9px; font-size: 21px; text-overflow: ellipsis; white-space: nowrap; }.overview-card > p { display: flex; gap: 12px; margin: 9px 0 0; color: #78879a; }.overview-icon { position: absolute; top: 17px; right: 17px; display: grid; width: 38px; height: 38px; place-items: center; border-radius: 12px; }.overview-icon.green { color: #07866f; background: #e1f5ef; }.overview-icon.blue { color: #2563eb; background: #e9f1ff; }.overview-icon.cyan { color: #0891b2; background: #e3f7fb; }.overview-icon.violet { color: #7c3aed; background: #f1eaff; }
.quick-card, .console-card, .version-manager-card, .logs-card { margin-top: 16px; padding: 18px; }.section-heading { display: flex; align-items: center; justify-content: space-between; gap: 14px; margin-bottom: 14px; }.section-heading h2 { margin: 0; font-size: 18px; }.section-heading p { margin: 4px 0 0; color: #718096; }.quick-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }.quick-grid button { display: flex; min-width: 0; align-items: center; gap: 13px; padding: 14px; border: 1px solid rgba(85, 113, 156, .16); border-radius: 12px; color: #172033; background: rgba(249, 252, 255, .86); text-align: left; cursor: pointer; }.quick-grid button:hover:not(:disabled) { border-color: rgba(15, 118, 110, .3); background: #f0fbf8; }.quick-grid button:disabled { opacity: .48; cursor: not-allowed; }.quick-grid button > span { display: grid; width: 38px; height: 38px; flex: 0 0 auto; place-items: center; border-radius: 11px; color: #0f766e; background: #e7f6f3; }.quick-grid button > div { display: grid; flex: 1; }.quick-grid small { margin-top: 3px; color: #718096; }
.quick-grid button.toggle-enabled { border-color: rgba(16, 185, 129, .24); background: rgba(240, 253, 250, .88); }.quick-toggle-state { display: inline-flex; flex: 0 0 auto; height: 24px; align-items: center; padding: 0 9px; border-radius: 999px; font-size: 10px; font-style: normal; font-weight: 820; }.quick-toggle-state.on { color: #047857; background: #d1fae5; }.quick-toggle-state.off { color: #64748b; background: #eef2f7; }
.console-output { overflow: auto; max-height: 280px; min-height: 150px; padding: 15px; border-radius: 12px; background: #111821; color: #b9f69b; font: 12.5px/1.65 ui-monospace, SFMono-Regular, Menlo, monospace; }.console-line { display: grid; grid-template-columns: 72px minmax(0, 1fr); gap: 8px; }.console-line time { color: #64748b; }.console-line.stderr span { color: #fda4af; }.console-line.system span { color: #93c5fd; }.console-empty { display: grid; min-height: 120px; place-items: center; color: #64748b; }.interactive-input { display: flex; gap: 10px; margin-top: 12px; }
.web-management-card { display: grid; justify-items: center; padding: 58px 28px; text-align: center; }.web-orb { display: grid; width: 82px; height: 82px; place-items: center; margin-bottom: 18px; border-radius: 26px; color: #0f766e; background: #e3f6f1; font-size: 34px; }.web-management-card h2 { margin: 14px 0 6px; font-size: 28px; }.web-management-card > p { color: #66758c; font: 16px ui-monospace, SFMono-Regular, Menlo, monospace; }.web-actions { display: flex; gap: 12px; margin: 24px 0; }
.vision-offline-card { display: grid; min-height: 430px; place-items: center; align-content: center; gap: 12px; padding: 48px 24px; border: 1px solid rgba(85, 113, 156, .17); border-radius: 16px; background: rgba(255, 255, 255, .82); text-align: center; }.vision-offline-card h2 { margin: 8px 0 0; font-size: 24px; }.vision-offline-card p { max-width: 560px; margin: 0 0 8px; color: #66758c; line-height: 1.7; }.vision-offline-icon { display: grid; width: 72px; height: 72px; place-items: center; border-radius: 20px; color: #0f766e; background: #e3f6f1; font-size: 30px; }
.vision-manager-card { overflow: hidden; border: 1px solid rgba(85, 113, 156, .17); border-radius: 16px; background: rgba(255, 255, 255, .86); box-shadow: 0 14px 34px rgba(30, 53, 84, .06); }.vision-manager-header { display: grid; grid-template-columns: minmax(0, 1fr) minmax(210px, 280px); align-items: center; gap: 24px; padding: 24px; border-bottom: 1px solid rgba(85, 113, 156, .13); background: linear-gradient(120deg, rgba(238, 250, 247, .94), rgba(242, 247, 255, .94)); }.vision-kicker { color: #0f766e; font-size: 11px; font-weight: 850; letter-spacing: .08em; }.vision-manager-header h2 { margin: 6px 0; font-size: 24px; }.vision-manager-header p { max-width: 700px; margin: 0; color: #607087; line-height: 1.65; }.vision-sidecar-summary { display: grid; gap: 4px; padding: 16px; border: 1px solid rgba(15, 118, 110, .16); border-radius: 12px; background: rgba(255, 255, 255, .78); }.vision-sidecar-summary small, .vision-sidecar-summary span { color: #718096; }.vision-sidecar-summary strong { overflow: hidden; color: #0f766e; font-size: 16px; text-overflow: ellipsis; white-space: nowrap; }
.vision-toolbar { display: grid; grid-template-columns: minmax(220px, 1fr) auto auto auto; gap: 8px; padding: 16px 18px; border-bottom: 1px solid rgba(85, 113, 156, .12); }.vision-model-list { display: grid; max-height: min(52vh, 560px); overflow-y: auto; padding: 8px 18px 18px; }.vision-model-row { display: grid; grid-template-columns: 34px minmax(0, 1fr) auto; min-width: 0; align-items: center; gap: 10px; min-height: 58px; padding: 8px 10px; border-bottom: 1px solid rgba(85, 113, 156, .1); cursor: pointer; }.vision-model-row:hover:not(.disabled):not(.native), .vision-model-row.selected { background: rgba(236, 253, 245, .74); }.vision-model-row.native { cursor: default; }.vision-model-row.disabled { opacity: .52; cursor: not-allowed; }.vision-native-check { display: grid; width: 22px; height: 22px; place-items: center; border-radius: 50%; color: #fff; background: #16a34a; font-size: 12px; }.vision-model-identity { display: grid; min-width: 0; gap: 3px; }.vision-model-identity strong, .vision-model-identity small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.vision-model-identity strong { color: #172033; font-size: 14px; }.vision-model-identity small { color: #718096; font-size: 11px; }.vision-mode-pill { display: inline-flex; min-width: 76px; height: 25px; align-items: center; justify-content: center; padding: 0 9px; border-radius: 999px; font-size: 10px; font-weight: 820; }.vision-mode-pill.native { color: #047857; background: #d1fae5; }.vision-mode-pill.sidecar { color: #1d4ed8; background: #dbeafe; }.vision-mode-pill.text { color: #64748b; background: #eef2f7; }.vision-mode-pill.disabled { color: #9f1239; background: #ffe4e6; }.vision-save-bar { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 16px 20px; border-top: 1px solid rgba(85, 113, 156, .14); background: #f8fafc; }.vision-save-bar > div { display: grid; gap: 3px; }.vision-save-bar span { color: #718096; font-size: 12px; }
.version-header-card { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); overflow: hidden; border: 1px solid rgba(85, 113, 156, .17); border-radius: 16px; background: rgba(255, 255, 255, .82); }.version-header-card > div { display: grid; gap: 6px; padding: 22px; }.version-header-card > div + div { border-left: 1px solid rgba(85, 113, 156, .14); }.version-header-card small, .version-header-card span { color: #718096; }.version-header-card strong { font-size: 26px; }.latest-release { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 16px; padding: 20px; border-radius: 14px; background: linear-gradient(135deg, #effaf7, #eef6ff); }.release-icon { display: grid; width: 58px; height: 58px; place-items: center; border-radius: 18px; color: #0f766e; background: #fff; font-size: 24px; }.latest-release h3 { margin: 8px 0 4px; font-size: 21px; }.latest-release p { margin: 0; color: #66758c; }.release-picker { display: flex; gap: 10px; margin-top: 14px; }.release-picker .arco-select { flex: 1; }
.local-version-section, .github-version-section { margin-top: 18px; padding-top: 18px; border-top: 1px solid rgba(85, 113, 156, .14); }.local-version-heading { display: flex; align-items: center; justify-content: space-between; gap: 14px; }.local-version-heading h3 { margin: 0; font-size: 16px; }.local-version-heading p { margin: 4px 0 0; color: #718096; }.local-version-list { display: grid; gap: 8px; margin-top: 12px; }.local-version-row { display: flex; min-width: 0; align-items: center; justify-content: space-between; gap: 12px; padding: 11px 12px; border: 1px solid rgba(85, 113, 156, .14); border-radius: 10px; background: rgba(248, 251, 255, .86); }.local-version-row > div:first-child { display: grid; gap: 3px; min-width: 0; }.local-version-row span { color: #718096; font-size: 12px; }.local-version-actions { display: flex; flex: 0 0 auto; gap: 6px; }
.full-log { max-height: calc(100vh - 330px); min-height: 420px; }.settings-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; }.settings-card { padding: 20px; }.settings-card :deep(.arco-input-number) { width: 100%; }.danger-card { grid-column: 1 / -1; border-color: rgba(245, 158, 11, .28); }.danger-actions { display: flex; gap: 10px; }
.scan-summary { display: flex; flex-wrap: wrap; gap: 8px; margin: 0; color: #52637b; }.scan-summary > span { display: inline-flex; height: 28px; align-items: center; gap: 4px; padding: 0 10px; border: 1px solid rgba(85, 113, 156, .16); border-radius: 999px; background: rgba(248, 250, 252, .9); font-size: 12px; }.scan-summary .scan-ready { color: #047857; border-color: rgba(16, 185, 129, .2); background: rgba(236, 253, 245, .92); }
.migration-list { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); max-height: 340px; gap: 10px; overflow: auto; margin: 14px 0; padding: 3px 6px 3px 3px; scrollbar-gutter: stable; }
.migration-account-card { position: relative; display: grid; grid-template-columns: auto auto minmax(0, 1fr) auto; align-items: center; gap: 11px; min-width: 0; min-height: 104px; padding: 13px 12px; border: 1px solid rgba(113, 128, 150, .18); border-radius: 8px; background: rgba(255, 255, 255, .96); box-shadow: 0 10px 24px rgba(28, 45, 74, .07); cursor: pointer; transition: border-color .18s ease, box-shadow .18s ease, transform .18s ease; }
.migration-account-card:hover:not(.unavailable) { border-color: rgba(37, 99, 235, .4); box-shadow: 0 16px 34px rgba(28, 45, 74, .11); transform: translateY(-1px); }.migration-account-card:focus-visible { outline: 2px solid rgba(37, 99, 235, .55); outline-offset: 2px; }.migration-account-card.selected { border-color: rgba(37, 99, 235, .76); box-shadow: 0 16px 38px rgba(37, 99, 235, .12), inset 0 0 0 1px rgba(37, 99, 235, .12); }.migration-account-card.current { background: linear-gradient(180deg, rgba(239, 246, 255, .96), rgba(255, 255, 255, .96)); }.migration-account-card.unavailable { cursor: default; }
.migration-check { display: inline-flex; width: 28px; height: 36px; align-items: center; justify-content: center; }.migration-avatar { display: grid; width: 42px; height: 42px; place-items: center; border-radius: 13px; color: #0f766e; background: linear-gradient(145deg, #dcf7ef, #e7f3ff); font-size: 15px; font-weight: 850; box-shadow: inset 0 0 0 1px rgba(15, 118, 110, .08); }.migration-account-card.selected .migration-avatar { color: #fff; background: linear-gradient(145deg, #2563eb, #0f766e); }
.migration-identity { display: grid; min-width: 0; gap: 5px; }.migration-account-title { display: flex; min-width: 0; align-items: center; gap: 6px; }.migration-account-title > strong { overflow: hidden; min-width: 0; color: #172033; font-size: 14px; text-overflow: ellipsis; white-space: nowrap; }.migration-identity > small { overflow: hidden; color: #6b7a90; font-size: 11px; font-weight: 700; text-overflow: ellipsis; white-space: nowrap; }.migration-reason { overflow: hidden; color: #718096; font-size: 12px; text-overflow: ellipsis; white-space: nowrap; }
.migration-current-pill, .migration-plan-pill, .migration-status-pill { display: inline-flex; flex: 0 0 auto; height: 22px; align-items: center; justify-content: center; padding: 0 8px; border-radius: 999px; font-size: 10px; font-weight: 800; line-height: 1; }.migration-current-pill { color: #1d4ed8; border: 1px solid rgba(37, 99, 235, .2); background: #eff6ff; }.migration-plan-pill { max-width: 74px; overflow: hidden; color: #0369a1; border: 1px solid rgba(14, 165, 233, .2); background: #f0f9ff; text-overflow: ellipsis; text-transform: uppercase; white-space: nowrap; }.migration-status-pill { color: #475569; background: #f1f5f9; }.migration-status-pill.ready { color: #047857; background: #ecfdf5; }.migration-status-pill.already_imported { color: #1d4ed8; background: #eff6ff; }.migration-status-pill.unsupported { color: #b45309; background: #fffbeb; }.migration-status-pill.invalid { color: #be123c; background: #fff1f2; }
.migration-card-actions { display: grid; min-width: 82px; justify-items: end; gap: 8px; }.migration-delete-button { color: #be123c; border-color: transparent; background: rgba(255, 241, 242, .94); }.migration-delete-button:hover:not(:disabled) { color: #fff; background: #e11d48; }
.migration-batch-actions { display: flex; flex-wrap: wrap; gap: 10px; }
@media (max-width: 1080px) { .opencodex-hero { align-items: flex-start; flex-direction: column; }.opencodex-status-strip { width: 100%; overflow-x: auto; }.overview-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
@media (max-width: 900px) { .migration-list { grid-template-columns: 1fr; } }
@media (max-width: 760px) { .opencodex-tabs { overflow-x: auto; }.opencodex-tabs button { flex: 0 0 auto; }.overview-grid, .quick-grid, .settings-grid, .version-header-card, .vision-manager-header { grid-template-columns: 1fr; }.version-header-card > div + div { border-top: 1px solid rgba(85, 113, 156, .14); border-left: 0; }.latest-release { grid-template-columns: 1fr; }.release-picker, .web-actions { align-items: stretch; flex-direction: column; }.local-version-row, .local-version-heading, .vision-save-bar { align-items: stretch; flex-direction: column; }.local-version-actions { justify-content: flex-end; }.migration-account-card { grid-template-columns: auto auto minmax(0, 1fr); }.migration-card-actions { grid-column: 2 / -1; display: flex; min-width: 0; align-items: center; justify-content: space-between; }.vision-toolbar { grid-template-columns: 1fr 1fr; }.vision-toolbar .arco-input-wrapper { grid-column: 1 / -1; } }
</style>
