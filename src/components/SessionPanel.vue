<script setup lang="ts">
import type { CodexSessionRecord, CodexTrashedSessionRecord } from "../services/session";
import type { SessionGroup } from "../types/ui";
import { formatLocalizedCount, t } from "../i18n";

defineProps<{
  sessionSearch: { titleQuery: string; contentQuery: string };
  sessionTrashMode: boolean;
  sessionLoading: boolean;
  backupWorking: boolean;
  backupButtonText: string;
  sessionBackupLoading: boolean;
  sessionRepairing: boolean;
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

defineEmits<{
  (event: "update:session-trash-mode", value: boolean): void;
  (event: "load-sessions"): void;
  (event: "toggle-all-sessions"): void;
  (event: "export-session-backup"): void;
  (event: "open-session-restore-modal"): void;
  (event: "repair-sessions"): void;
  (event: "trash-sessions"): void;
  (event: "restore-sessions"): void;
  (event: "toggle-session-group-expanded", key: string): void;
  (event: "toggle-session-group-selection", group: SessionGroup): void;
  (event: "toggle-session", id: string): void;
  (event: "open-session-folder", path: string): void;
}>();

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
</script>

<template>
  <section class="session-panel">
    <div class="session-toolbar">
      <div class="session-search">
        <a-input
          v-model="sessionSearch.titleQuery"
          allow-clear
          :placeholder="t('搜索会话标题')"
          @press-enter="$emit('load-sessions')"
        />
        <a-input
          v-model="sessionSearch.contentQuery"
          allow-clear
          :placeholder="t('搜索会话内容')"
          @press-enter="$emit('load-sessions')"
        />
      </div>
      <div class="session-actions">
        <a-button
          :disabled="!activeSessionIds.length"
          @click="$emit('toggle-all-sessions')"
        >
          {{ allSessionsSelected ? t("取消全选") : (sessionTrashMode ? t("全选回收站") : t("全选")) }}
        </a-button>
        <a-button
          :type="sessionTrashMode ? 'secondary' : 'primary'"
          @click="() => { $emit('update:session-trash-mode', false); $emit('load-sessions'); }"
        >
          {{ t("会话列表") }}
        </a-button>
        <a-button
          :type="sessionTrashMode ? 'primary' : 'secondary'"
          @click="() => { $emit('update:session-trash-mode', true); $emit('load-sessions'); }"
        >
          {{ t("回收站") }}
        </a-button>
        <a-button :loading="sessionLoading" @click="$emit('load-sessions')">
          <template #icon><icon-refresh /></template>
          {{ t("刷新") }}
        </a-button>
        <a-button :loading="backupWorking" @click="$emit('export-session-backup')">
          <template #icon><icon-download /></template>
          {{ backupButtonText }}
        </a-button>
        <a-button :loading="sessionBackupLoading" :disabled="backupWorking" @click="$emit('open-session-restore-modal')">
          <template #icon><icon-import /></template>
          {{ t("恢复会话") }}
        </a-button>
        <a-button :loading="sessionRepairing" type="primary" @click="$emit('repair-sessions')">
          <template #icon><icon-tool /></template>
          {{ t("修复可见性") }}
        </a-button>
        <a-button
          v-if="!sessionTrashMode"
          status="danger"
          :disabled="!selectedSessionIdList.length"
          @click="$emit('trash-sessions')"
        >
          <template #icon><icon-delete /></template>
          {{ t("移入回收站") }}
        </a-button>
        <a-button
          v-else
          type="primary"
          :disabled="!selectedSessionIdList.length"
          @click="$emit('restore-sessions')"
        >
          <template #icon><icon-undo /></template>
          {{ t("恢复") }}
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
              @click="$emit('toggle-session-group-expanded', group.key)"
            >
              <template #icon>
                <icon-down v-if="expandedSessionGroups.has(group.key)" />
                <icon-right v-else />
              </template>
            </a-button>
            <a-checkbox
              :model-value="isSessionGroupSelected(group)"
              @change="$emit('toggle-session-group-selection', group)"
            />
            <icon-folder class="session-group-icon" />
            <button
              class="session-group-title"
              type="button"
              :title="group.projectName"
              @click="$emit('toggle-session-group-expanded', group.key)"
            >
              {{ group.projectName }}
            </button>
            <span class="session-group-meta">{{ formatLocalizedCount(group.sessions.length, "条会话") }}</span>
            <span class="token-count">{{ new Intl.NumberFormat("en-US").format(group.approximateTokens) }} tokens</span>
            <span class="session-size">{{ formatFileSize(group.sizeBytes) }}</span>
            <span class="session-group-time">{{ formatTime(group.latestUpdatedAt) }}</span>
          </div>
          <div v-if="expandedSessionGroups.has(group.key)" class="session-group-children">
            <article v-for="session in group.sessions" :key="session.id" class="session-child-row">
              <a-checkbox
                :model-value="selectedSessionIds.has(session.id)"
                @change="$emit('toggle-session', session.id)"
              />
              <div class="session-main session-main-name-only">
                <strong class="session-name-only" :title="session.title">
                  {{ session.title || t("未命名会话") }}
                </strong>
              </div>
              <div class="session-stat">
                <span class="token-count">{{ sessionApproxTokens(session.id) }}</span>
                <span class="session-size">{{ formatFileSize(session.sizeBytes) }}</span>
                <span>{{ formatTime(session.updatedAt) }}</span>
                <a-button size="small" @click="$emit('open-session-folder', session.path)">
                  <template #icon><icon-folder /></template>
                  {{ t("打开文件夹") }}
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
            <strong>{{ sessionSearch.titleQuery || sessionSearch.contentQuery ? t("没有匹配的会话") : t("还没有可显示的会话") }}</strong>
            <span>
              {{ sessionSearch.titleQuery || sessionSearch.contentQuery
                ? t("换个关键词试试，或清空搜索后重新刷新。")
                : t("可以先刷新本机会话；如果是切号后看不到旧会话，使用修复可见性重新挂回列表。")
              }}
            </span>
          </div>
          <div class="session-empty-actions">
            <a-button type="primary" :loading="sessionLoading" @click="$emit('load-sessions')">
              <template #icon><icon-refresh /></template>
              {{ t("刷新会话") }}
            </a-button>
            <a-button :loading="sessionBackupLoading" :disabled="backupWorking" @click="$emit('open-session-restore-modal')">
              <template #icon><icon-import /></template>
              {{ t("从备份恢复") }}
            </a-button>
            <a-button :loading="sessionRepairing" @click="$emit('repair-sessions')">
              <template #icon><icon-tool /></template>
              {{ t("修复可见性") }}
            </a-button>
          </div>
        </div>
      </div>

      <div v-else class="session-list">
        <article v-for="session in trashedSessions" :key="session.id" class="session-row">
          <a-checkbox
            :model-value="selectedSessionIds.has(session.id)"
            @change="$emit('toggle-session', session.id)"
          />
          <div class="session-main">
            <strong :title="session.title">{{ session.title }}</strong>
            <span :title="session.originalPath">{{ session.originalPath }}</span>
          </div>
          <div class="session-stat">
            <span>{{ t("已删除") }}</span>
            <span>{{ formatTime(session.deletedAt) }}</span>
            <a-button size="small" @click="$emit('open-session-folder', session.originalPath)">
              <template #icon><icon-folder /></template>
              {{ t("打开文件夹") }}
            </a-button>
          </div>
        </article>
        <div v-if="!trashedSessions.length" class="session-empty-state compact">
          <div class="session-empty-icon">
            <icon-delete />
          </div>
          <div class="session-empty-copy">
            <strong>{{ t("回收站为空") }}</strong>
            <span>{{ t("被移入回收站的会话会显示在这里，恢复后会回到原来的会话路径。") }}</span>
          </div>
        </div>
      </div>
    </a-spin>
  </section>
</template>
