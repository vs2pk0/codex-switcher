<script setup lang="ts">
import type { CodexSwitcherBackupFile } from "../services/codex";
import { t } from "../i18n";

defineProps<{
  visible: boolean;
  backups: CodexSwitcherBackupFile[];
  loading: boolean;
  backupWorking: boolean;
}>();

defineEmits<{
  (event: "update:visible", visible: boolean): void;
  (event: "restore", backup: CodexSwitcherBackupFile): void;
  (event: "backup-now"): void;
}>();
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('恢复会话数据')"
    :footer="false"
    width="720px"
    @update:visible="$emit('update:visible', $event)"
  >
    <a-spin :loading="loading" dot>
      <div v-if="backups.length" class="session-restore-list">
        <article v-for="backup in backups" :key="backup.path" class="session-restore-item">
          <div class="session-restore-main">
            <strong>{{ backup.name }}</strong>
            <span>{{ backup.createdAt }}</span>
          </div>
          <a-button type="primary" :disabled="backupWorking" @click="$emit('restore', backup)">
            <template #icon><icon-import /></template>
            {{ t("只恢复会话") }}
          </a-button>
        </article>
      </div>
      <div v-else class="session-empty-state compact">
        <div class="session-empty-icon">
          <icon-archive />
        </div>
        <div class="session-empty-copy">
          <strong>{{ t("还没有备份文件") }}</strong>
          <span>{{ t("先备份一次会话数据，之后就可以从这里只恢复会话。") }}</span>
        </div>
        <div class="session-empty-actions">
          <a-button type="primary" :loading="backupWorking" @click="$emit('backup-now')">
            <template #icon><icon-download /></template>
            {{ t("立即备份") }}
          </a-button>
        </div>
      </div>
    </a-spin>
  </a-modal>
</template>
