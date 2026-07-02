<script setup lang="ts">
import { t } from "../i18n";

defineProps<{
  visible: boolean;
  title: string;
  progress: number;
  status: "running" | "completed" | "failed";
  message: string;
}>();

defineEmits<{
  (event: "update:visible", visible: boolean): void;
}>();
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t(title)"
    :footer="false"
    :closable="true"
    :mask-closable="true"
    width="420px"
    @update:visible="$emit('update:visible', $event)"
  >
    <div class="backup-progress-panel">
      <a-progress
        :percent="progress / 100"
        :status="status === 'failed' ? 'danger' : status === 'completed' ? 'success' : 'normal'"
      />
      <div
        class="backup-progress-message"
        :class="{ failed: status === 'failed' }"
      >
        {{ t(message) }}
      </div>
      <a-button
        v-if="status !== 'running'"
        type="primary"
        long
        @click="$emit('update:visible', false)"
      >
        {{ t("关闭") }}
      </a-button>
    </div>
  </a-modal>
</template>
