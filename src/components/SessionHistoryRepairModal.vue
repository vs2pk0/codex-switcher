<script setup lang="ts">
import { ref, watch } from "vue";
import { t } from "../i18n";
import type { CodexSessionRecord } from "../services/session";

const props = defineProps<{
  visible: boolean;
  session: CodexSessionRecord | null;
  repairing: boolean;
}>();
const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "repair", recoverContext: boolean): void;
}>();
const recoverContext = ref(false);
watch(() => props.visible, (visible) => { if (visible) recoverContext.value = false; });
function updateVisible(visible: boolean): void {
  if (!props.repairing) emit("update:visible", visible);
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('恢复完整会话')"
    :footer="false"
    :closable="!repairing"
    :mask-closable="!repairing"
    :esc-to-close="!repairing"
    width="560px"
    @update:visible="updateVisible"
  >
    <div class="history-repair-options">
      <strong>{{ session?.title || t("未命名会话") }}</strong>
      <p>{{ t("将修复该会话的分页历史、消息序号、不兼容的消息与工具 ID 和本地索引。原始文件会自动备份，消息正文、工具记录与模型设置会保留；修复期间对应 Codex 实例会重启，请先等待正在运行的任务结束。") }}</p>
      <a-checkbox v-model="recoverContext" :disabled="repairing">
        {{ t("恢复超限上下文") }}
      </a-checkbox>
      <a-alert v-if="recoverContext" type="warning">
        {{ t("适用于上下文超限且 /compact 失败的会话。保留完整原始记录和备份，仅将模型上下文替换为最近对话的有限文本摘录；这不是 AI 摘要，早期细节、图片和工具输出不会继续发送给模型，必要时请重新提供。") }}
      </a-alert>
      <div class="session-mutation-actions">
        <a-button :disabled="repairing" @click="updateVisible(false)">{{ t("取消") }}</a-button>
        <a-button type="primary" :loading="repairing" @click="emit('repair', recoverContext)">
          {{ t("开始修复") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>

<style scoped>
.history-repair-options { display: flex; flex-direction: column; gap: 16px; }
.history-repair-options p { margin: 0; line-height: 1.7; color: var(--color-text-2); }
</style>
