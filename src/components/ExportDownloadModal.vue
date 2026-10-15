<script setup lang="ts">
import { t } from "../i18n";
import type { ExportDownloadResult } from "../services/codex";

defineProps<{
  result: ExportDownloadResult | null;
  opening: boolean;
}>();

defineEmits<{
  (event: "close"): void;
  (event: "open-folder"): void;
}>();
</script>

<template>
  <a-modal
    :visible="Boolean(result)"
    :title="t('下载完成')"
    :width="620"
    :footer="false"
    @cancel="$emit('close')"
  >
    <div v-if="result" class="export-download-result">
      <p class="download-success"><icon-check-circle-fill />{{ t("文件已保存到以下位置") }}</p>
      <a-button
        type="text"
        class="download-file"
        :title="t('打开文件夹')"
        :disabled="opening"
        @click="$emit('open-folder')"
      >
        <template #icon><icon-file /></template>
        {{ result.fileName }}
      </a-button>
      <div class="download-path">
        <span>{{ t("保存位置") }}</span>
        <code>{{ result.path }}</code>
      </div>
      <div class="download-actions">
        <a-button @click="$emit('close')">{{ t("关闭") }}</a-button>
        <a-button type="primary" :loading="opening" @click="$emit('open-folder')">
          <template #icon><icon-folder /></template>
          {{ t("打开文件夹") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>

<style scoped>
.export-download-result { display: grid; gap: 16px; }
.download-success { display: flex; align-items: center; gap: 8px; margin: 0; }
.download-success :deep(svg) { color: rgb(var(--success-6)); font-size: 22px; }
.download-file { justify-content: flex-start; height: auto; min-height: 36px; white-space: normal; overflow-wrap: anywhere; text-align: left; }
.download-path { display: grid; gap: 8px; padding: 14px; border-radius: 8px; background: var(--color-fill-1); }
.download-path span { color: var(--color-text-3); font-size: 12px; }
.download-path code { white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; }
.download-actions { display: flex; justify-content: flex-end; gap: 12px; }
</style>
