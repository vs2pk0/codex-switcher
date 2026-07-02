<script setup lang="ts">
import { t } from "../i18n";
import type { CodexExportFormat } from "../services/codex";

defineProps<{
  visible: boolean;
  title: string;
  exportFormat: CodexExportFormat;
  exportFormatOptions: Array<{ label: string; value: CodexExportFormat }>;
  previewVisible: boolean;
  text: string;
  summary: string;
  width?: string;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "update:export-format", value: CodexExportFormat): void;
  (event: "update:preview-visible", value: boolean): void;
  (event: "format-change"): void;
  (event: "copy"): void;
  (event: "download"): void;
}>();

function handleFormatChange(value: unknown): void {
  emit("update:export-format", value as CodexExportFormat);
  emit("format-change");
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="title"
    :footer="false"
    :width="width || '760px'"
    @update:visible="$emit('update:visible', $event)"
  >
    <div class="modal-form">
      <div class="export-toolbar">
        <div class="export-format">
          <span>{{ t("导出格式") }}</span>
          <a-select
            :model-value="exportFormat"
            size="large"
            @change="handleFormatChange"
          >
            <a-option
              v-for="option in exportFormatOptions"
              :key="option.value"
              :value="option.value"
            >
              {{ option.label }}
            </a-option>
          </a-select>
        </div>
        <div>
          <a-button @click="$emit('update:preview-visible', !previewVisible)">
            <template #icon><icon-eye /></template>
            {{ previewVisible ? t("隐藏预览") : t("预览") }}
          </a-button>
          <a-button @click="$emit('copy')">
            <template #icon><icon-copy /></template>
            {{ t("复制") }}
          </a-button>
          <a-button type="primary" @click="$emit('download')">
            <template #icon><icon-download /></template>
            {{ t("下载") }}
          </a-button>
        </div>
      </div>
      <a-textarea
        :model-value="previewVisible ? text : summary"
        class="token-textarea export-json-viewer"
        :class="{ collapsed: !previewVisible }"
        readonly
        :auto-size="previewVisible ? { minRows: 14, maxRows: 24 } : { minRows: 12, maxRows: 12 }"
      />
    </div>
  </a-modal>
</template>
