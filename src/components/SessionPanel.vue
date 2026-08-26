<script setup lang="ts">
import { computed } from "vue";
import type { CodexSessionRecord, CodexTrashedSessionRecord } from "../services/session";
import { instanceDisplayName, type CodexInstance } from "../services/instances";
import type { SessionGroup } from "../types/ui";
import { formatLocalizedCount, t } from "../i18n";

const props = defineProps<{
  instances: CodexInstance[];
  selectedInstanceId: string;
  sessionSearch: { titleQuery: string; contentQuery: string };
  sessionTrashMode: boolean;
  sessionLoading: boolean;
  backupWorking: boolean;
  backupButtonText: string;
  sessionBackupLoading: boolean;
  sessionRepairing: boolean;
  repairingSessionId: string;
  sessionModelRepairing: boolean;
  activeSessionIds: string[];
  allSessionsSelected: boolean;
  selectedSessionIds: Set<string>;
  selectedSessionIdList: string[];
  expandedSessionGroups: Set<string>;
  sessionGroups: SessionGroup[];
  sessions: CodexSessionRecord[];
  trashedSessions: CodexTrashedSessionRecord[];
  formatTime: (value?: string | number | null) => string;
  sessionApproxTokens: (sessionId: string) => string;
  isSessionGroupSelected: (group: SessionGroup) => boolean;
}>();

const emit = defineEmits<{
  (event: "select-instance", instanceId: string): void;
  (event: "update:session-trash-mode", value: boolean): void;
  (event: "load-sessions"): void;
  (event: "toggle-all-sessions"): void;
  (event: "export-session-backup"): void;
  (event: "open-session-restore-modal"): void;
  (event: "repair-sessions"): void;
  (event: "repair-session-history", session: CodexSessionRecord): void;
  (event: "repair-session-models"): void;
  (event: "trash-sessions"): void;
  (event: "restore-sessions"): void;
  (event: "toggle-session-group-expanded", key: string): void;
  (event: "toggle-session-group-selection", group: SessionGroup): void;
  (event: "toggle-session", id: string): void;
  (event: "open-session-folder", path: string): void;
  (event: "view-session-content", session: CodexSessionRecord): void;
  (event: "copy-session", session: CodexSessionRecord): void;
  (event: "rename-session", session: CodexSessionRecord): void;
  (event: "edit-session-directory", session: CodexSessionRecord): void;
}>();

const totalTokens = computed(() => props.sessionGroups.reduce((sum, group) => sum + group.approximateTokens, 0));
const totalSize = computed(() => props.sessionGroups.reduce((sum, group) => sum + group.sizeBytes, 0));
const selectedInstance = computed(() =>
  props.instances.find((instance) => instance.id === props.selectedInstanceId),
);

function formatFileSize(bytes?: number | null): string {
  const safeBytes = typeof bytes === "number" && Number.isFinite(bytes) ? bytes : 0;
  if (safeBytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = safeBytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const digits = value >= 10 || unitIndex === 0 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unitIndex]}`;
}

function formatTokens(value: number): string {
  return new Intl.NumberFormat("en-US").format(value);
}

function switchMode(trashMode: boolean): void {
  emit("update:session-trash-mode", trashMode);
  emit("load-sessions");
}
</script>

<template>
  <section class="session-panel session-workspace">
    <section v-if="instances.length > 1" class="session-instance-context">
      <div class="session-instance-copy">
        <span>{{ t("当前会话实例") }}</span>
        <strong>{{ selectedInstance ? instanceDisplayName(selectedInstance) : t("系统默认实例") }}</strong>
        <small>{{ selectedInstance?.codexHome || "~/.codex" }}</small>
        <small class="session-instance-policy">
          {{ selectedInstance?.isDefault
            ? t("官方实例会包含在默认完整备份中，也可以在这里单独手动备份。")
            : t("多开实例不会进入默认完整备份，只能在这里手动备份。") }}
        </small>
      </div>
      <a-select
        :model-value="selectedInstanceId"
        :disabled="sessionLoading || backupWorking || sessionRepairing || sessionModelRepairing"
        popup-container="body"
        class="session-instance-select"
        @change="emit('select-instance', String($event))"
      >
        <a-option v-for="instance in instances" :key="instance.id" :value="instance.id">
          <div class="session-instance-option">
            <strong>{{ instanceDisplayName(instance) }}</strong>
            <span>{{ instance.running ? t("运行中") : t("未运行") }} · {{ instance.codexHome }}</span>
          </div>
        </a-option>
      </a-select>
    </section>

    <section class="session-overview-grid">
      <article>
        <span class="session-overview-icon blue"><icon-folder /></span>
        <div><small>{{ t("项目") }}</small><strong>{{ sessionGroups.length }} <em>{{ t("个") }}</em></strong></div>
      </article>
      <article>
        <span class="session-overview-icon green"><icon-message /></span>
        <div><small>{{ t("会话") }}</small><strong>{{ sessions.length }} <em>{{ t("条") }}</em></strong></div>
      </article>
      <article>
        <span class="session-overview-icon violet"><icon-storage /></span>
        <div><small>Tokens</small><strong>{{ formatTokens(totalTokens) }} <em>tokens</em></strong></div>
      </article>
      <article>
        <span class="session-overview-icon orange"><icon-computer /></span>
        <div><small>{{ t("磁盘占用") }}</small><strong>{{ formatFileSize(totalSize) }}</strong></div>
      </article>
    </section>

    <div class="session-toolbar session-command-bar">
      <div class="session-search">
        <a-input v-model="sessionSearch.titleQuery" allow-clear :placeholder="t('搜索会话标题')" @press-enter="emit('load-sessions')">
          <template #prefix><icon-search /></template>
        </a-input>
        <a-input v-model="sessionSearch.contentQuery" allow-clear :placeholder="t('搜索会话内容')" @press-enter="emit('load-sessions')">
          <template #prefix><icon-search /></template>
        </a-input>
      </div>
      <div class="session-view-switch">
        <a-button :type="sessionTrashMode ? 'secondary' : 'primary'" @click="switchMode(false)">{{ t("会话列表") }}</a-button>
        <a-button :type="sessionTrashMode ? 'primary' : 'secondary'" @click="switchMode(true)">{{ t("回收站") }}</a-button>
      </div>
      <div class="session-actions">
        <a-button :loading="sessionLoading" @click="emit('load-sessions')"><template #icon><icon-refresh /></template>{{ t("刷新") }}</a-button>
        <a-button :loading="backupWorking" @click="emit('export-session-backup')"><template #icon><icon-download /></template>{{ backupButtonText }}</a-button>
        <a-button :loading="sessionBackupLoading" :disabled="backupWorking" @click="emit('open-session-restore-modal')"><template #icon><icon-import /></template>{{ t("恢复会话") }}</a-button>
        <a-button :loading="sessionRepairing" type="primary" @click="emit('repair-sessions')"><template #icon><icon-tool /></template>{{ t("修复可见性") }}</a-button>
        <a-button :loading="sessionModelRepairing" status="success" @click="emit('repair-session-models')"><template #icon><icon-sync /></template>{{ t("一键修复切号会话") }}</a-button>
        <a-button v-if="!sessionTrashMode" status="danger" :disabled="!selectedSessionIdList.length" @click="emit('trash-sessions')"><template #icon><icon-delete /></template>{{ t("移入回收站") }}</a-button>
        <template v-else>
          <a-button :disabled="!activeSessionIds.length" @click="emit('toggle-all-sessions')"><template #icon><icon-check /></template>{{ t(allSessionsSelected ? "取消全选" : "全选回收站") }}</a-button>
          <a-button type="primary" :disabled="!selectedSessionIdList.length" @click="emit('restore-sessions')"><template #icon><icon-undo /></template>{{ t("恢复") }}</a-button>
        </template>
      </div>
    </div>

    <a-spin :loading="sessionLoading" dot>
      <div v-if="!sessionTrashMode" class="session-list session-project-table">
        <div class="session-table-head">
          <a-checkbox :model-value="allSessionsSelected" :disabled="!activeSessionIds.length" @change="emit('toggle-all-sessions')" />
          <span>{{ t("项目名称") }}</span><span>{{ t("会话") }}</span><span>Tokens</span><span>{{ t("占用") }}</span><span>{{ t("更新时间") }}</span><span>{{ t("操作") }}</span>
        </div>
        <section v-for="group in sessionGroups" :key="group.key" class="session-group">
          <div class="session-group-row">
            <a-button class="session-expand-button" size="mini" shape="circle" :title="expandedSessionGroups.has(group.key) ? t('收起') : t('展开')" @click="emit('toggle-session-group-expanded', group.key)">
              <template #icon><icon-down v-if="expandedSessionGroups.has(group.key)" /><icon-right v-else /></template>
            </a-button>
            <a-checkbox :model-value="isSessionGroupSelected(group)" @change="emit('toggle-session-group-selection', group)" />
            <icon-folder class="session-group-icon" />
            <button class="session-group-title" type="button" :title="group.key" @click="emit('toggle-session-group-expanded', group.key)">{{ group.projectName }}</button>
            <span class="session-group-meta">{{ formatLocalizedCount(group.sessions.length, "条会话") }}</span>
            <span class="token-count">{{ formatTokens(group.approximateTokens) }}</span>
            <span class="session-size">{{ formatFileSize(group.sizeBytes) }}</span>
            <span class="session-group-time">{{ formatTime(group.latestUpdatedAt) }}</span>
            <a-button class="session-more-button" size="mini" type="text" :title="t('展开会话')" @click="emit('toggle-session-group-expanded', group.key)"><template #icon><icon-more /></template></a-button>
          </div>
          <div v-if="expandedSessionGroups.has(group.key)" class="session-group-children">
            <article v-for="session in group.sessions" :key="session.id" class="session-child-row">
              <a-checkbox :model-value="selectedSessionIds.has(session.id)" @change="emit('toggle-session', session.id)" />
              <strong class="session-name-only" :title="session.title">{{ session.title || t("未命名会话") }}</strong>
              <span class="token-count">{{ sessionApproxTokens(session.id) }}</span>
              <span class="session-size">{{ formatFileSize(session.sizeBytes) }}</span>
              <span>{{ formatTime(session.updatedAt) }}</span>
              <div class="session-child-actions">
                <a-tooltip :content="t('恢复完整会话')">
                  <a-button
                    size="small"
                    type="text"
                    status="success"
                    :loading="repairingSessionId === session.id"
                    :disabled="Boolean(repairingSessionId && repairingSessionId !== session.id) || sessionRepairing || sessionModelRepairing"
                    @click="emit('repair-session-history', session)"
                  >
                    <template #icon><icon-tool /></template>
                  </a-button>
                </a-tooltip>
                <a-tooltip :content="t('查看会话内容')">
                  <a-button size="small" type="text" @click="emit('view-session-content', session)"><template #icon><icon-eye /></template></a-button>
                </a-tooltip>
                <a-tooltip :content="t('复制到其他实例或目录')">
                  <a-button size="small" type="text" @click="emit('copy-session', session)"><template #icon><icon-copy /></template></a-button>
                </a-tooltip>
                <a-tooltip :content="t('修改会话名称')">
                  <a-button size="small" type="text" @click="emit('rename-session', session)"><template #icon><icon-edit /></template></a-button>
                </a-tooltip>
                <a-tooltip :content="t('修改工作目录')">
                  <a-button size="small" type="text" @click="emit('edit-session-directory', session)"><template #icon><icon-folder-add /></template></a-button>
                </a-tooltip>
                <a-tooltip :content="t('打开文件夹')">
                  <a-button size="small" type="text" @click="emit('open-session-folder', session.path)"><template #icon><icon-folder /></template></a-button>
                </a-tooltip>
              </div>
            </article>
          </div>
        </section>
        <div v-if="!sessions.length" class="session-empty-state">
          <div class="session-empty-icon"><icon-message /></div>
          <div class="session-empty-copy"><strong>{{ sessionSearch.titleQuery || sessionSearch.contentQuery ? t("没有匹配的会话") : t("还没有可显示的会话") }}</strong><span>{{ sessionSearch.titleQuery || sessionSearch.contentQuery ? t("换个关键词试试，或清空搜索后重新刷新。") : t("可以先刷新本机会话；如果是切号后看不到旧会话，使用修复可见性重新挂回列表。") }}</span></div>
          <div class="session-empty-actions"><a-button type="primary" :loading="sessionLoading" @click="emit('load-sessions')"><template #icon><icon-refresh /></template>{{ t("刷新会话") }}</a-button><a-button :loading="sessionBackupLoading" :disabled="backupWorking" @click="emit('open-session-restore-modal')"><template #icon><icon-import /></template>{{ t("从备份恢复") }}</a-button><a-button :loading="sessionRepairing" @click="emit('repair-sessions')"><template #icon><icon-tool /></template>{{ t("修复可见性") }}</a-button></div>
        </div>
        <footer v-if="sessions.length" class="session-table-footer"><span><icon-check-circle /> {{ t("已选择") }} {{ selectedSessionIdList.length }} {{ t("项") }}</span><div><span>{{ t("共") }} {{ sessions.length }} {{ t("条会话") }}</span><span>{{ t("总 Tokens") }} {{ formatTokens(totalTokens) }}</span><span>{{ t("总占用") }} {{ formatFileSize(totalSize) }}</span></div></footer>
      </div>

      <div v-else class="session-list session-trash-table">
        <article v-for="session in trashedSessions" :key="session.id" class="session-row">
          <a-checkbox :model-value="selectedSessionIds.has(session.id)" @change="emit('toggle-session', session.id)" />
          <div class="session-main"><strong :title="session.title">{{ session.title }}</strong><span :title="session.originalPath">{{ session.originalPath }}</span></div>
          <div class="session-stat"><span>{{ t("已删除") }}</span><span>{{ formatTime(session.deletedAt) }}</span><a-button size="small" @click="emit('open-session-folder', session.originalPath)"><template #icon><icon-folder /></template>{{ t("打开文件夹") }}</a-button></div>
        </article>
        <div v-if="!trashedSessions.length" class="session-empty-state compact"><div class="session-empty-icon"><icon-delete /></div><div class="session-empty-copy"><strong>{{ t("回收站为空") }}</strong><span>{{ t("被移入回收站的会话会显示在这里，恢复后会回到原来的会话路径。") }}</span></div></div>
      </div>
    </a-spin>
  </section>
</template>
