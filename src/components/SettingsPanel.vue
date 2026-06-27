<script setup lang="ts">
import type {
  CodexSwitcherBackupFile,
  CodexSwitcherPaths,
  CodexSwitcherSettings,
} from "../services/codex";

const props = defineProps<{
  settings: CodexSwitcherSettings;
  appPaths: CodexSwitcherPaths | null;
  backups: CodexSwitcherBackupFile[];
  loading: boolean;
  saving: boolean;
  backupLoading: boolean;
  backupWorking: boolean;
}>();

const emit = defineEmits<{
  (event: "save"): void;
  (event: "open-path", path: string): void;
  (event: "reset-config"): void;
  (event: "export-backup"): void;
  (event: "restore-backup", backup: CodexSwitcherBackupFile): void;
  (event: "delete-backup", backup: CodexSwitcherBackupFile): void;
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

</script>

<template>
  <section class="settings-panel">
    <a-spin :loading="loading" dot>
      <div class="settings-grid">
        <a-card title="数据" :bordered="false" class="settings-card">
          <div class="path-row">
            <span>应用目录</span>
            <a-input :model-value="appPaths?.appDir || ''" readonly />
            <a-button @click="openPath(appPaths?.appDir)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>账号 JSON</span>
            <a-input :model-value="appPaths?.accountsJson || ''" readonly />
            <a-button @click="openPath(appPaths?.accountsJson)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>账号目录</span>
            <a-input :model-value="appPaths?.accountDir || ''" readonly />
            <a-button @click="openPath(appPaths?.accountDir)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>会话目录</span>
            <a-input :model-value="appPaths?.sessionDir || ''" readonly />
            <a-button @click="openPath(appPaths?.sessionDir)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>配置目录</span>
            <a-input :model-value="appPaths?.dataDir || ''" readonly />
            <a-button @click="openPath(appPaths?.dataDir)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>设置 JSON</span>
            <a-input :model-value="appPaths?.settingsJson || ''" readonly />
            <a-button @click="openPath(appPaths?.settingsJson)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>Codex 目录</span>
            <a-input :model-value="appPaths?.codexHome || ''" readonly />
            <a-button @click="openPath(appPaths?.codexHome)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
          <div class="path-row">
            <span>备份目录</span>
            <a-input :model-value="appPaths?.backupDir || ''" readonly />
            <a-button @click="openPath(appPaths?.backupDir)">
              <template #icon><icon-folder /></template>
            </a-button>
          </div>
        </a-card>

        <a-card title="刷新" :bordered="false" class="settings-card">
          <a-form :model="settings" layout="vertical">
            <a-form-item label="监控额度">
              <a-switch v-model="settings.monitorQuota" @change="emit('save')" />
            </a-form-item>
            <a-form-item label="额度自动刷新">
              <a-input-number
                v-model="settings.quotaRefreshMinutes"
                :min="1"
                :max="120"
                mode="button"
              >
                <template #suffix>分钟</template>
              </a-input-number>
            </a-form-item>
            <a-form-item label="当前账号刷新">
              <a-input-number
                v-model="settings.currentAccountRefreshMinutes"
                :min="1"
                :max="120"
                mode="button"
              >
                <template #suffix>分钟</template>
              </a-input-number>
            </a-form-item>
            <a-form-item label="等待刷新倒计时">
              <a-switch v-model="settings.showQuotaCountdowns" @change="emit('save')" />
            </a-form-item>
            <a-button type="primary" :loading="saving" @click="emit('save')">保存</a-button>
          </a-form>
        </a-card>

        <a-card title="外观" :bordered="false" class="settings-card settings-appearance">
          <a-form :model="settings" layout="vertical">
            <a-form-item label="每行固定账号数">
              <a-radio-group v-model="settings.maxColumns" type="button" @change="emit('save')">
                <a-radio :value="3">3 个</a-radio>
                <a-radio :value="4">4 个</a-radio>
                <a-radio :value="5">5 个</a-radio>
              </a-radio-group>
            </a-form-item>
            <a-form-item label="每页账号数">
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

        <a-card title="配置" :bordered="false" class="settings-card">
          <p class="settings-hint">
            重置会删除本机 Codex 目录下的 config.toml，适合切换配置异常时恢复默认配置。
          </p>
          <a-button status="danger" @click="emit('reset-config')">
            <template #icon><icon-delete /></template>
            重置 config.toml
          </a-button>
        </a-card>
        <a-card title="备份" :bordered="false" class="settings-card settings-backup">
          <div class="backup-actions">
            <a-button type="primary" :loading="backupWorking" @click="emit('export-backup')">
              <template #icon><icon-download /></template>
              手动备份
            </a-button>
            <a-button @click="openPath(appPaths?.backupDir)">
              <template #icon><icon-folder /></template>
              打开备份目录
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
                  <a-popconfirm
                    content="确认使用这个 ZIP 备份恢复账号与设置？"
                    @ok="emit('restore-backup', backup)"
                  >
                    <a-button :disabled="backupWorking">
                      <template #icon><icon-import /></template>
                      恢复
                    </a-button>
                  </a-popconfirm>
                  <a-popconfirm
                    content="确认删除这个备份文件？"
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
                <strong>还没有备份</strong>
                <span>点击“手动备份”会把账号、设置、徽章、排序与会话数据打包成 ZIP。</span>
                <code>{{ appPaths?.backupDir || "~/.codex_switcher/backup" }}</code>
              </div>
            </div>
          </a-spin>
        </a-card>
      </div>
    </a-spin>
  </section>
</template>
