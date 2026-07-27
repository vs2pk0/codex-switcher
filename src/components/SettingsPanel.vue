<script setup lang="ts">
import type {
  CodexConfigFileKind,
  CodexSwitcherBackupFile,
  CodexSwitcherPaths,
  CodexSwitcherSettings,
} from "../services/codex";
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
}>();

const emit = defineEmits<{
  (event: "save"): void;
  (event: "open-path", path: string): void;
  (event: "edit-codex-file", fileKind: CodexConfigFileKind): void;
  (event: "reset-config"): void;
  (event: "export-backup"): void;
  (event: "refresh-backups"): void;
  (event: "restore-backup", backup: CodexSwitcherBackupFile): void;
  (event: "delete-backup", backup: CodexSwitcherBackupFile): void;
  (event: "open-push-settings"): void;
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
  return t(`手动备份 ${progress}%`);
}

function changeLanguage(value: unknown): void {
  if (typeof value !== "string") return;
  props.settings.language = value;
  setLanguage(value);
  emit("save");
}

</script>

<template>
  <section class="settings-panel">
    <a-spin :loading="loading" dot>
      <div class="settings-grid">
        <a-card :title="t('数据')" :bordered="false" class="settings-card settings-data">
          <div class="path-row">
            <span>{{ t("应用目录") }}</span>
            <a-input :model-value="appPaths?.appDir || ''" readonly />
            <a-button @click="openPath(appPaths?.appDir)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>{{ t("账号 JSON") }}</span>
            <a-input :model-value="appPaths?.accountsJson || ''" readonly />
            <a-button @click="openPath(appPaths?.accountsJson)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>{{ t("账号目录") }}</span>
            <a-input :model-value="appPaths?.accountDir || ''" readonly />
            <a-button @click="openPath(appPaths?.accountDir)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>{{ t("会话目录") }}</span>
            <a-input :model-value="appPaths?.sessionDir || ''" readonly />
            <a-button @click="openPath(appPaths?.sessionDir)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>{{ t("统计目录") }}</span>
            <a-input :model-value="appPaths?.statisticsDir || ''" readonly />
            <a-button @click="openPath(appPaths?.statisticsDir)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>{{ t("配置目录") }}</span>
            <a-input :model-value="appPaths?.dataDir || ''" readonly />
            <a-button @click="openPath(appPaths?.dataDir)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>{{ t("设置 JSON") }}</span>
            <a-input :model-value="appPaths?.settingsJson || ''" readonly />
            <a-button @click="openPath(appPaths?.settingsJson)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>{{ t("Codex 目录") }}</span>
            <a-input :model-value="appPaths?.codexHome || ''" readonly />
            <a-button @click="openPath(appPaths?.codexHome)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>{{ t("备份目录") }}</span>
            <a-input :model-value="appPaths?.backupDir || ''" readonly />
            <a-button @click="openPath(appPaths?.backupDir)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
        </a-card>

        <a-card :title="t('刷新')" :bordered="false" class="settings-card settings-refresh">
          <a-form :model="settings" layout="vertical">
            <a-form-item :label="t('显示 GPT 5.3 Codex Spark 额度')">
              <a-switch v-model="settings.showAdditionalQuotaWindows" @change="emit('save')" />
            </a-form-item>
            <a-form-item :label="t('额度自动刷新')">
              <a-input-number
                v-model="settings.quotaRefreshMinutes"
                :min="1"
                :max="1440"
                mode="button"
              >
                <template #suffix>{{ t("分钟") }}</template>
              </a-input-number>
            </a-form-item>
            <a-form-item :label="t('当前账号刷新')">
              <a-input-number
                v-model="settings.currentAccountRefreshMinutes"
                :min="1"
                :max="1440"
                mode="button"
              >
                <template #suffix>{{ t("分钟") }}</template>
              </a-input-number>
            </a-form-item>
            <a-form-item :label="t('等待刷新倒计时')">
              <a-switch v-model="settings.showQuotaCountdowns" @change="emit('save')" />
            </a-form-item>
            <a-button type="primary" :loading="saving" @click="emit('save')">{{ t("保存") }}</a-button>
          </a-form>
        </a-card>

        <a-card :title="t('外观')" :bordered="false" class="settings-card settings-appearance">
          <a-form :model="settings" layout="vertical">
            <a-form-item :label="t('语言')">
              <a-select
                :model-value="settings.language || 'zh-CN'"
                popup-container="body"
                :scrollbar="false"
                :trigger-props="{ contentClass: 'account-filter-dropdown language-dropdown' }"
                @change="changeLanguage"
              >
                <a-option v-for="item in languageOptions" :key="item.value" :value="item.value">
                  {{ item.label }}
                </a-option>
              </a-select>
            </a-form-item>
            <a-form-item :label="t('侧边栏')">
              <a-switch v-model="settings.sidebarEnabled" @change="emit('save')" />
            </a-form-item>
            <a-form-item :label="t('每行固定账号数')">
              <a-radio-group v-model="settings.maxColumns" type="button" @change="emit('save')">
                <a-radio :value="3">{{ t("3 个") }}</a-radio>
                <a-radio :value="4">{{ t("4 个") }}</a-radio>
                <a-radio :value="5">{{ t("5 个") }}</a-radio>
              </a-radio-group>
            </a-form-item>
            <a-form-item :label="t('每页账号数')">
              <a-input-number
                v-model="settings.pageSize"
                :min="10"
                :max="500"
                :step="10"
                mode="button"
                @change="emit('save')"
              />
            </a-form-item>
          </a-form>
        </a-card>

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
            <a-button status="danger" @click="emit('reset-config')">
              <template #icon><icon-delete /></template>
              {{ t("重置 config.toml") }}
            </a-button>
          </div>
        </a-card>
        <a-card :title="t('推送')" :bordered="false" class="settings-card settings-push">
          <div class="settings-config-actions">
            <a-button type="primary" @click="emit('open-push-settings')">
              <template #icon><icon-notification /></template>
              {{ t("推送设置") }}
            </a-button>
          </div>
        </a-card>
        <a-card :title="t('备份')" :bordered="false" class="settings-card settings-backup">
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
          <a-spin :loading="backupLoading" dot>
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
                <span>{{ t("点击“手动备份”会把账号、设置、统计缓存、费用规则与所有 Codex 会话记录打包成 ZIP。") }}</span>
                <code>{{ appPaths?.backupDir || "~/.codex_switcher/backup" }}</code>
              </div>
            </div>
          </a-spin>
        </a-card>
      </div>
    </a-spin>
  </section>
</template>
