<script setup lang="ts">
import { ref, watch } from "vue";
import { Message } from "@arco-design/web-vue";
import { open } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";
import type { CodexSessionRecord } from "../services/session";

interface SessionCopyDirectoryOption {
  name: string;
  path: string;
}

const props = defineProps<{
  visible: boolean;
  source: CodexSessionRecord | null;
  directories: SessionCopyDirectoryOption[];
  saving: boolean;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "save", projectPath: string): void;
}>();

const projectPath = ref("");

watch(
  () => [props.visible, props.source?.id],
  ([visible]) => {
    if (visible) projectPath.value = "";
  },
);

function updateVisible(visible: boolean): void {
  if (!props.saving) emit("update:visible", visible);
}

async function chooseDirectory(): Promise<void> {
  try {
    const selected = await open({
      multiple: false,
      directory: true,
      defaultPath: projectPath.value || props.source?.projectPath || undefined,
    });
    if (typeof selected === "string" && selected) projectPath.value = selected;
  } catch (error) {
    Message.error(`${t("选择工作目录失败")}：${String(error)}`);
  }
}

function submit(): void {
  const targetPath = projectPath.value.trim();
  if (!targetPath) {
    Message.warning(t("请选择副本要归属的工作目录"));
    return;
  }
  emit("save", targetPath);
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('创建会话副本')"
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
      <a-alert type="info">
        {{ t("副本会显示在目标目录分组中，并与该目录已有会话共存；源会话及其他已有会话不会被修改。") }}
      </a-alert>
      <label class="session-copy-directory-field">
        <span>{{ t("目标工作目录") }}</span>
        <div class="session-copy-directory-input">
          <a-select
            v-model="projectPath"
            allow-search
            allow-clear
            :disabled="saving"
            :placeholder="t('从已有项目选择')"
            popup-container="body"
            :trigger-props="{ contentClass: 'session-copy-select-popup' }"
          >
            <a-option v-for="directory in directories" :key="directory.path" :value="directory.path">
              <div class="session-copy-option-content">
                <strong class="session-copy-directory-name">{{ directory.name }}</strong>
                <span class="session-copy-project-path">{{ directory.path }}</span>
              </div>
            </a-option>
          </a-select>
          <a-button :disabled="saving" @click="chooseDirectory">
            <template #icon><icon-folder /></template>
            {{ t("选择其他目录") }}
          </a-button>
        </div>
        <small v-if="projectPath" class="session-copy-selected-path">{{ projectPath }}</small>
      </label>
      <div class="session-copy-target">
        <span>{{ t("新会话名称") }}</span>
        <strong>{{ source?.title || t("未命名会话") }} {{ t("副本") }}</strong>
      </div>
      <div class="session-mutation-actions">
        <a-button :disabled="saving" @click="updateVisible(false)">{{ t("取消") }}</a-button>
        <a-button type="primary" :loading="saving" :disabled="!projectPath.trim()" @click="submit">
          <template #icon><icon-copy /></template>
          {{ t("创建会话副本") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
