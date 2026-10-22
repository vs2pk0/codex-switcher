<script setup lang="ts">
import { computed, ref } from "vue";
import type {
  CodexConfigFileKind,
  CodexSwitcherBackupFile,
  CodexSwitcherPaths,
  CodexSwitcherSettings,
} from "../services/codex";
import type { CodexInstance } from "../services/instances";
import { instanceDisplayName } from "../services/instances";
import { languageOptions, setLanguage, t } from "../i18n";

const props = defineProps<{
  settings: CodexSwitcherSettings;
  appPaths: CodexSwitcherPaths | null;
  backups: CodexSwitcherBackupFile[];
  loading: boolean;
  saving: boolean;
  backupLoading: boolean;
  backupWorking: boolean;
  backupProgress: number;
  instances: CodexInstance[];
  selectedInstanceId: string;
}>();

const emit = defineEmits<{
  (event: "save"): void;
  (event: "open-path", path: string): void;
  (event: "edit-codex-file", fileKind: CodexConfigFileKind): void;
  (event: "reset-config"): void;
  (event: "delete-config"): void;
  (event: "export-backup"): void;
  (event: "refresh-backups"): void;
  (event: "restore-backup", backup: CodexSwitcherBackupFile): void;
  (event: "delete-backup", backup: CodexSwitcherBackupFile): void;
  (event: "open-push-settings"): void;
  (event: "open-reset-records"): void;
  (event: "select-instance", instanceId: string): void;
}>();

function openPath(path?: string): void {
  if (path) emit("open-path", path);
}

function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value >= 10 || index === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[index]}`;
}

function backupButtonText(): string {
  if (!props.backupWorking) return t("手动备份");
  const progress = Math.max(0, Math.min(100, Math.round(props.backupProgress || 0)));
  return `${t("手动备份")} ${progress}%`;
}

function changeLanguage(value: unknown): void {
  if (typeof value !== "string") return;
  props.settings.language = value;
  setLanguage(value);
  emit("save");
}

const showAllPaths = ref(false);
const showBackups = ref(false);
const selectedInstance = computed(() => props.instances.find(item => item.id === props.selectedInstanceId));
const directoryRows = computed(() => {
  const paths = props.appPaths;
  const primary = [
    { label: "应用目录", path: paths?.appDir },
    { label: "账号 JSON", path: paths?.accountsJson },
    { label: "Codex 目录", path: paths?.codexHome },
    { label: "备份目录", path: paths?.backupDir },
  ];
  return showAllPaths.value ? [...primary,
    { label: "账号目录", path: paths?.accountDir },
    { label: "会话目录", path: paths?.sessionDir },
    { label: "统计目录", path: paths?.statisticsDir },
    { label: "配置目录", path: paths?.dataDir },
    { label: "设置 JSON", path: paths?.settingsJson },
  ] : primary;
});

function displayPath(path?: string): string {
  if (!path) return "—";
  const normalized = path.replaceAll("\\", "/");
  if (props.appPaths?.appDir && path !== props.appPaths.appDir) {
    const root = props.appPaths.appDir.replaceAll("\\", "/").replace(/\/$/, "");
    if (normalized.startsWith(`${root}/`)) return normalized.slice(root.length + 1);
  }
  return normalized.replace(/^\/Users\/[^/]+(?=\/|$)/, "~").replace(/^[A-Za-z]:\/Users\/[^/]+(?=\/|$)/, "~");
}

</script>

<template>
  <section class="settings-panel settings-redesign">
    <header class="settings-heading"><h1>{{ t("设置") }}</h1><p>{{ t("管理偏好、账号切换与本地数据") }}</p></header>
    <a-spin :loading="loading" dot>
      <a-card :bordered="false" class="settings-card instance-settings-card">
        <div class="instance-settings-row">
          <div class="instance-settings-copy"><strong>{{ t("当前设置实例") }}</strong><p>{{ t("下方 Codex 目录和配置文件操作只作用于所选实例。") }}</p></div>
          <div class="instance-settings-picker"><a-select :aria-label="t('当前设置实例')" :model-value="selectedInstanceId" class="instance-settings-select" popup-container="body" @change="(value: unknown) => typeof value === 'string' && emit('select-instance', value)">
            <a-option v-for="instance in instances" :key="instance.id" :value="instance.id">{{ t(instanceDisplayName(instance)) }}</a-option>
          </a-select></div>
          <span v-if="selectedInstance" class="instance-status" :class="{ running: selectedInstance.running }"><i />{{ selectedInstance.running ? t("运行中") : t("未运行") }}</span>
        </div>
      </a-card>
      <div class="settings-main-grid">
        <div class="settings-column">
          <a-card :bordered="false" class="settings-card settings-switching">
            <div class="section-heading"><div><h2>{{ t("账号切换") }}</h2><p>{{ t("控制切换账号时的行为，避免不必要的配置变更。") }}</p></div><span class="default-pill">{{ t("默认关闭") }}</span></div>
            <div class="preference-row"><div class="preference-copy"><strong>{{ t('切换时重置默认配置') }}</strong><p>{{ t('关闭时保留现有配置，仅更新登录凭据与必要路由。') }}</p></div><div class="preference-control"><a-switch v-model="settings.resetConfigOnAccountSwitch" :aria-label="t('切换时重置默认配置')" :disabled="saving" @change="emit('save')" /></div></div><div class="preference-row"><div class="preference-copy"><strong>{{ t('切换时自动修复会话') }}</strong><p>{{ t('关闭时保留会话文件和索引，需要时可手动修复。') }}</p></div><div class="preference-control"><a-switch v-model="settings.repairSessionsOnAccountSwitch" :aria-label="t('切换时自动修复会话')" :disabled="saving" @change="emit('save')" /></div></div>
          </a-card>
          <a-card :bordered="false" class="settings-card settings-refresh">
            <div class="section-heading"><div><h2>{{ t("额度与刷新") }}</h2><p>{{ t("管理额度显示与账号数据刷新策略。") }}</p></div></div>
            <div class="preference-row"><div class="preference-copy"><strong>{{ t('显示 GPT 5.3 Codex Spark 额度') }}</strong></div><div class="preference-control"><a-switch v-model="settings.showAdditionalQuotaWindows" :aria-label="t('显示 GPT 5.3 Codex Spark 额度')" :disabled="saving" @change="emit('save')" /></div></div><div class="preference-row"><div class="preference-copy"><strong>{{ t('额度自动刷新') }}</strong></div><div class="preference-control"><a-input-number :aria-label="t('额度自动刷新')" v-model="settings.quotaRefreshMinutes" :min="1" :max="518400" mode="button" ><template #suffix>{{ t("分钟") }}</template></a-input-number></div></div><div class="preference-row"><div class="preference-copy"><strong>{{ t('当前账号刷新') }}</strong></div><div class="preference-control"><a-input-number :aria-label="t('当前账号刷新')" v-model="settings.currentAccountRefreshMinutes" :min="1" :max="518400" mode="button" ><template #suffix>{{ t("分钟") }}</template></a-input-number></div></div><div class="preference-row"><div class="preference-copy"><strong>{{ t('等待刷新倒计时') }}</strong></div><div class="preference-control"><a-switch v-model="settings.showQuotaCountdowns" :aria-label="t('等待刷新倒计时')" :disabled="saving" @change="emit('save')" /></div></div><div class="preference-row"><div class="preference-copy"><strong>{{ t('自动续期登录令牌') }}</strong><p>{{ t('关闭后不再后台自动刷新 OAuth Token，手动操作仍可按需续期。') }}</p></div><div class="preference-control"><a-switch v-model="settings.tokenKeeperEnabled" :aria-label="t('自动续期登录令牌')" :disabled="saving" @change="emit('save')" /></div></div>
            <div class="settings-save-row"><a-button type="primary" :loading="saving" @click="emit('save')">{{ t("保存刷新设置") }}</a-button></div>
          </a-card>
        </div>
        <div class="settings-column">
          <a-card :bordered="false" class="settings-card settings-appearance">
            <div class="section-heading"><div><h2>{{ t("外观") }}</h2><p>{{ t("自定义界面显示与布局。") }}</p></div></div>
            <div class="preference-row"><div class="preference-copy"><strong>{{ t('语言') }}</strong></div><div class="preference-control"><a-select :aria-label="t('语言')" :model-value="settings.language || 'zh-CN'" popup-container="body" :scrollbar="false" @change="changeLanguage"><a-option v-for="item in languageOptions" :key="item.value" :value="item.value">{{ item.label }}</a-option></a-select></div></div><div class="preference-row"><div class="preference-copy"><strong>{{ t('侧边栏') }}</strong></div><div class="preference-control"><a-switch v-model="settings.sidebarEnabled" :aria-label="t('侧边栏')" :disabled="saving" @change="emit('save')" /></div></div><div class="preference-row"><div class="preference-copy"><strong>{{ t('每行账号数') }}</strong></div><div class="preference-control"><a-radio-group :aria-label="t('每行账号数')" v-model="settings.maxColumns" type="button" @change="emit('save')"><a-radio :value="0">{{ t("自适应") }}</a-radio><a-radio :value="3">3</a-radio><a-radio :value="4">4</a-radio><a-radio :value="5">5</a-radio></a-radio-group></div></div><div class="preference-row"><div class="preference-copy"><strong>{{ t('每页账号数') }}</strong></div><div class="preference-control"><a-input-number :aria-label="t('每页账号数')" v-model="settings.pageSize" :step="10" :min="10" :max="500" mode="button" @change="emit('save')"></a-input-number></div></div>
          </a-card>
          <a-card :bordered="false" class="settings-card settings-data">
            <div class="section-heading"><div><h2>{{ t("本地数据") }}</h2><p>{{ t("路径仅用于显示，点击文件夹按钮打开原始位置。") }}</p></div></div>
            <div v-for="item in directoryRows" :key="item.label" class="directory-row">
              <span>{{ t(item.label) }}</span><code :title="item.path">{{ displayPath(item.path) }}</code>
              <a-button type="text" :disabled="!item.path" :aria-label="`${t('打开')} ${t(item.label)}`" @click="openPath(item.path)"><template #icon><icon-folder /></template></a-button>
            </div>
            <a-button type="text" class="directory-disclosure" @click="showAllPaths = !showAllPaths">{{ showAllPaths ? t("收起目录") : t("展开全部目录") }}<icon-up v-if="showAllPaths" /><icon-down v-else /></a-button>
          </a-card>
        </div>
      </div>
      <div class="settings-utilities">
        <a-card :title="t('配置')" :bordered="false" class="settings-card settings-config">
          <p class="settings-hint">
            {{ t("直接查看并编辑本机 Codex 配置文件，保存前会检查格式并自动备份原文件。") }}
          </p>
          <div class="settings-config-actions">
            <a-button type="primary" @click="emit('edit-codex-file', 'auth')">
              <template #icon><icon-code /></template>
              {{ t("编辑 auth.json") }}
            </a-button>
            <a-button @click="emit('edit-codex-file', 'config')">
              <template #icon><icon-edit /></template>
              {{ t("编辑 config.toml") }}
            </a-button>
            <a-divider />
            <a-button status="warning" @click="emit('reset-config')">
              <template #icon><icon-undo /></template>
              {{ t("重置 config.toml") }}
            </a-button>
            <a-button status="danger" @click="emit('delete-config')">
              <template #icon><icon-delete /></template>
              {{ t("删除 config.toml") }}
            </a-button>
          </div>
        </a-card>

        <a-card :title="t('备份与恢复')" :bordered="false" class="settings-card settings-backup">
          <a-alert type="info">
            {{ t("默认完整备份只包含账号、设置等工具数据和官方默认实例的会话，不会备份任何多开实例。多开实例请在“会话管理”中切换实例后手动备份。") }}
          </a-alert>
          <div class="backup-actions">
            <a-button type="primary" :loading="backupWorking" @click="emit('export-backup')">
              <template #icon><icon-download /></template>
              {{ backupButtonText() }}
            </a-button>
            <a-button @click="openPath(appPaths?.backupDir)">
              <template #icon><icon-folder /></template>
              {{ t("打开备份目录") }}
            </a-button>
            <a-button :loading="backupLoading" @click="emit('refresh-backups')">
              <template #icon><icon-refresh /></template>
              {{ t("刷新") }}
            </a-button>
          </div>
          <a-button class="backup-disclosure" type="text" @click="showBackups = !showBackups">
            {{ showBackups ? t("收起备份列表") : t("查看备份") }} <icon-up v-if="showBackups" /><icon-down v-else />
          </a-button>
          <a-spin v-if="showBackups" :loading="backupLoading" dot>
            <div v-if="backups.length" class="backup-list">
              <article v-for="backup in backups" :key="backup.path" class="backup-item">
                <div class="backup-main">
                  <div class="backup-name">{{ backup.name }}</div>
                  <div class="backup-meta">
                    <span>{{ backup.createdAt }}</span>
                    <span>{{ formatFileSize(backup.sizeBytes) }}</span>
                  </div>
                </div>
                <div class="backup-item-actions">
                  <a-button :disabled="backupWorking" @click="emit('restore-backup', backup)">
                    <template #icon><icon-import /></template>
                    {{ t("恢复") }}
                  </a-button>
                  <a-popconfirm
                    :content="t('确认删除这个备份文件？')"
                    :ok-text="t('确认')"
                    :cancel-text="t('取消')"
                    @ok="emit('delete-backup', backup)"
                  >
                    <a-button status="danger" :disabled="backupWorking">
                      <template #icon><icon-delete /></template>
                    </a-button>
                  </a-popconfirm>
                </div>
              </article>
            </div>
            <div v-else class="backup-empty">
              <div class="backup-empty-icon">
                <icon-archive />
              </div>
              <div class="backup-empty-copy">
                <strong>{{ t("还没有备份") }}</strong>
                <span>{{ t("点击“手动备份”会把账号、设置、统计缓存、费用规则与官方默认实例的 Codex 会话记录打包成 ZIP；多开实例不会包含在内。") }}</span>
                <code>{{ appPaths?.backupDir || "~/.codex_switcher/backup" }}</code>
              </div>
            </div>
          </a-spin>
        </a-card>

        <a-card :title="t('推送与重置')" :bordered="false" class="settings-card settings-push">
          <div class="settings-config-actions">
            <a-button type="primary" @click="emit('open-push-settings')">
              <template #icon><icon-notification /></template>
              {{ t("推送设置") }}
            </a-button>
            <a-button @click="emit('open-reset-records')">
              <template #icon><icon-thunderbolt /></template>
              {{ t("重置记录") }}
            </a-button>
          </div>
        </a-card>

      </div>
    </a-spin>
  </section>
</template>

<style scoped>
.settings-redesign { max-width: 1600px; margin: 0 auto; padding-bottom: 24px; }
.settings-heading { margin: 0 0 20px; }
.settings-heading h1 { margin: 0 0 5px; font-size: 26px; color: #172033; letter-spacing: -.5px; }
.settings-heading p, .section-heading p, .instance-settings-copy p { margin: 5px 0 0; color: #7a879c; font-size: 13px; line-height: 1.6; }
.settings-redesign :deep(.arco-spin) { width: 100%; }
.settings-redesign .settings-card { height: auto; grid-column: auto; border-radius: 12px; box-shadow: 0 4px 18px rgba(20, 35, 55, .025); }
.settings-redesign :deep(.arco-card-body) { padding: 20px 24px; }
.settings-redesign :deep(.arco-card-header) { height: auto; min-height: 54px; padding: 16px 24px; border-bottom: 0; }
.settings-redesign :deep(.arco-card-header + .arco-card-body) { padding-top: 0; }
.instance-settings-card { margin-bottom: 20px; }
.instance-settings-row { display: flex; align-items: center; gap: 24px; }
.instance-settings-copy { max-width: 340px; }
.instance-settings-copy strong { color: #172033; font-size: 14px; }
.instance-settings-picker { width: 300px; flex: 0 1 300px; min-width: 0; }
.instance-settings-picker :deep(.arco-select) { width: 100%; }
.instance-status { display: flex; gap: 8px; align-items: center; white-space: nowrap; color: #8792a2; font-size: 13px; }
.instance-status i { width: 8px; height: 8px; border-radius: 50%; background: currentColor; }
.instance-status.running { color: #21a65d; }
.settings-main-grid { display: grid; grid-template-columns: minmax(0, 1.25fr) minmax(0, 1fr); align-items: stretch; gap: 20px; }
.settings-column { display: contents; }
/* 两列共享网格行，让对应卡片随最高内容自然等高。 */
.settings-main-grid .settings-switching { grid-area: 1 / 1; }
.settings-main-grid .settings-appearance { grid-area: 1 / 2; }
.settings-main-grid .settings-refresh { grid-area: 2 / 1; }
.settings-main-grid .settings-data { grid-area: 2 / 2; }
.section-heading { display: flex; align-items: start; justify-content: space-between; gap: 12px; margin-bottom: 12px; }
.section-heading h2 { margin: 0; color: #172033; font-size: 16px; }
.default-pill { padding: 4px 8px; border-radius: 6px; background: #f1f4f8; color: #7a879c; white-space: nowrap; font-size: 12px; }
.preference-row { display: flex; justify-content: space-between; align-items: center; gap: 20px; padding: 15px 0; border-bottom: 1px solid #edf0f5; }
.preference-row:last-child { border-bottom: 0; padding-bottom: 0; }
.preference-copy { min-width: 0; }
.preference-copy strong { color: #28364b; font-size: 14px; font-weight: 550; line-height: 1.5; }
.preference-copy p { margin: 4px 0 0; color: #8390a4; font-size: 12px; line-height: 1.6; max-width: 450px; }
.preference-control { flex-shrink: 0; display: flex; align-items: center; justify-content: flex-end; }
.preference-control :deep(.arco-input-number), .preference-control :deep(.arco-select) { width: 216px; }
.preference-control :deep(.arco-radio-group-button) { white-space: nowrap; }
.settings-switching .preference-row { align-items: center; }
.settings-save-row { display: flex; justify-content: flex-end; margin-top: 16px; }
.directory-row { display: grid; grid-template-columns: 80px minmax(0, 1fr) 30px; gap: 10px; align-items: center; padding: 7px 0; }
.directory-row > span { font-size: 13px; color: #425168; }
.directory-row code { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 8px 10px; background: #f5f7fa; border-radius: 6px; color: #59677e; font-size: 12px; font-family: ui-monospace, SFMono-Regular, Consolas, monospace; }
.directory-disclosure { margin-top: 10px; width: 100%; display: flex; justify-content: space-between; color: #64748b; border-top: 1px solid #edf0f5; border-radius: 0; }
.settings-utilities { display: grid; grid-template-columns: 1fr 1.35fr .85fr; gap: 20px; margin-top: 20px; align-items: stretch; }
.settings-redesign .settings-config-actions { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
.settings-redesign .settings-config-actions .arco-btn { justify-content: center; font-size: 12px; padding: 0 8px; }
.settings-config-actions :deep(.arco-divider) { grid-column: 1 / -1; }
.settings-push .settings-config-actions { grid-template-columns: 1fr; }
.settings-hint { margin: 0 0 14px; color: #8390a4; line-height: 1.6; font-size: 12px; }
.settings-backup :deep(.arco-alert) { font-size: 12px; line-height: 1.6; }
.backup-actions { gap: 8px; }
.backup-actions :deep(.arco-btn) { padding: 0 10px; font-size: 12px; }
.backup-disclosure { margin-bottom: 8px; }
.backup-item { flex-wrap: wrap; gap: 10px; }
@media (max-width: 1250px) {
  .settings-main-grid { grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr); }
  .settings-redesign :deep(.arco-card-body) { padding: 18px; }
  .preference-row { gap: 12px; }
  .preference-control :deep(.arco-input-number), .preference-control :deep(.arco-select) { width: 174px; }
  .settings-utilities { grid-template-columns: 1fr 1.2fr; }
  .settings-push { grid-column: 1 / -1 !important; }
  .settings-push .settings-config-actions { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
@media (max-width: 960px) {
  .settings-main-grid, .settings-utilities { grid-template-columns: minmax(0, 1fr); }
  .settings-main-grid .settings-card { grid-area: auto; }
  .instance-settings-row { flex-wrap: wrap; gap: 14px; }
  .instance-settings-copy { max-width: none; flex-basis: 100%; }
  .instance-settings-picker { width: min(300px, 70%); }
}
@media (max-width: 480px) {
  .preference-row { flex-wrap: wrap; }
  .preference-control { margin-left: auto; }
  .directory-row { grid-template-columns: 70px minmax(0, 1fr) 28px; gap: 6px; }
}
</style>
