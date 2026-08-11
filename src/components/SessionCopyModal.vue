<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Message } from "@arco-design/web-vue";
import { t } from "../i18n";
import type { CodexSessionRecord } from "../services/session";

const props = defineProps<{
  visible: boolean;
  source: CodexSessionRecord | null;
  sessions: CodexSessionRecord[];
  saving: boolean;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "save", targetSessionId: string): void;
}>();

const targetSessionId = ref("");
const targetSessions = computed(() =>
  props.sessions.filter((session) => session.id !== props.source?.id),
);
const selectedTarget = computed(() =>
  targetSessions.value.find((session) => session.id === targetSessionId.value) ?? null,
);

function sessionTitle(session: CodexSessionRecord): string {
  return session.title || t("未命名会话");
}

function sessionDirectoryName(session: CodexSessionRecord): string {
  const projectName = session.projectName.trim();
  if (projectName) return projectName;
  const normalizedPath = session.projectPath.replace(/[\\/]+$/, "");
  return normalizedPath.split(/[\\/]/).pop() || "-";
}

function sessionSearchLabel(session: CodexSessionRecord): string {
  return [sessionTitle(session), sessionDirectoryName(session), session.projectPath]
    .filter(Boolean)
    .join(" · ");
}

watch(
  () => [props.visible, props.source?.id],
  ([visible]) => {
    if (visible) targetSessionId.value = "";
  },
);

function updateVisible(visible: boolean): void {
  if (!props.saving) emit("update:visible", visible);
}

function submit(): void {
  if (!targetSessionId.value) {
    Message.warning(t("请选择目标会话"));
    return;
  }
  emit("save", targetSessionId.value);
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('复制会话数据')"
    :footer="false"
    :closable="!saving"
    :mask-closable="!saving"
    width="760px"
    modal-class="session-copy-modal"
    @update:visible="updateVisible"
  >
    <div class="session-copy-modal-body">
      <div class="session-copy-source">
        <span>{{ t("源会话") }}</span>
        <strong>{{ source?.title || t("未命名会话") }}</strong>
        <small>{{ source?.projectPath || source?.projectName }}</small>
      </div>
      <a-alert type="warning">
        {{ t("复制后会保留目标会话的新身份，但目标会话现有内容将被源会话历史覆盖；源会话不会改变，并会自动备份目标会话。") }}
      </a-alert>
      <label class="session-copy-target">
        <span>{{ t("目标会话") }}</span>
        <a-select
          v-model="targetSessionId"
          allow-search
          popup-container="body"
          :trigger-props="{ contentClass: 'session-copy-select-popup' }"
          :disabled="saving"
          :placeholder="t('请选择新建的空会话')"
        >
          <a-option
            v-for="session in targetSessions"
            :key="session.id"
            :value="session.id"
            :label="sessionSearchLabel(session)"
          >
            <div class="session-copy-option-content">
              <strong>{{ sessionTitle(session) }}</strong>
              <span class="session-copy-directory-name">
                {{ t("目录名称") }}：{{ sessionDirectoryName(session) }}
              </span>
              <span class="session-copy-project-path">{{ session.projectPath }}</span>
            </div>
          </a-option>
        </a-select>
        <div v-if="selectedTarget" class="session-copy-selected">
          <strong>{{ sessionTitle(selectedTarget) }}</strong>
          <span class="session-copy-directory-name">
            {{ t("目录名称") }}：{{ sessionDirectoryName(selectedTarget) }}
          </span>
          <span class="session-copy-project-path">{{ selectedTarget.projectPath }}</span>
        </div>
      </label>
      <div class="session-mutation-actions">
        <a-button :disabled="saving" @click="updateVisible(false)">{{ t("取消") }}</a-button>
        <a-button type="primary" status="warning" :loading="saving" :disabled="!targetSessionId" @click="submit">
          <template #icon><icon-copy /></template>
          {{ t("确认复制") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
