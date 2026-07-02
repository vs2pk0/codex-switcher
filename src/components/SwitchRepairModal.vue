<script setup lang="ts">
import type { CodexSessionVisibilityRepairSummary } from "../services/session";
import { t } from "../i18n";

defineProps<{
  visible: boolean;
  targetName: string;
  progress: number;
  result: CodexSessionVisibilityRepairSummary | null;
  error: string;
}>();

defineEmits<{
  (event: "update:visible", visible: boolean): void;
  (event: "close"): void;
}>();
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('Codex 会话不可见')"
    :footer="false"
    width="860px"
    modal-class="repair-modal"
    @update:visible="$emit('update:visible', $event)"
  >
    <div class="repair-body">
      <p class="repair-desc">
        {{ t("检测到 Codex 已切换到") }} {{ targetName }}。{{ t("由于官方机制，这类切换后原有会话可能不会自动显示，正在自动修复会话可见性。") }}
      </p>
      <div class="repair-progress-line">
        <strong>{{ t("修复进度") }}</strong>
        <span>{{ progress }}%</span>
      </div>
      <a-progress :percent="progress" :show-text="false" />
      <div v-if="result" class="repair-result success">
        <strong>{{ t("修复已完成") }}</strong>
        <span>{{ result.message }}</span>
      </div>
      <div v-else-if="error" class="repair-result error">
        <strong>{{ t("修复失败") }}</strong>
        <span>{{ error }}</span>
      </div>
      <div class="form-actions">
        <a-button type="primary" @click="$emit('close')">{{ t("关闭") }}</a-button>
      </div>
    </div>
  </a-modal>
</template>
