<script setup lang="ts">
import { ref, watch } from "vue";
import { Message } from "@arco-design/web-vue";
import { open } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";
import type { CodexSessionRecord } from "../services/session";

const props = defineProps<{
  visible: boolean;
  session: CodexSessionRecord | null;
  saving: boolean;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "save", projectPath: string): void;
}>();

const projectPath = ref("");

watch(
  () => [props.visible, props.session?.id],
  ([visible]) => {
    if (visible) projectPath.value = props.session?.projectPath || "";
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
      defaultPath: projectPath.value || undefined,
    });
    if (typeof selected === "string" && selected) projectPath.value = selected;
  } catch (error) {
    Message.error(`${t("选择工作目录失败")}：${String(error)}`);
  }
}

function submit(): void {
  const nextPath = projectPath.value.trim();
  if (!nextPath) {
    Message.warning(t("工作目录不能为空"));
    return;
  }
  emit("save", nextPath);
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('修改工作目录')"
    :footer="false"
    :closable="!saving"
    :mask-closable="!saving"
    width="620px"
    modal-class="session-directory-modal"
    @update:visible="updateVisible"
  >
    <div class="session-directory-modal-body">
      <div class="session-directory-session">
        <span>{{ t("会话名称") }}</span>
        <strong>{{ session?.title || t("未命名会话") }}</strong>
      </div>
      <a-alert type="warning">
        {{ t("修改后，此会话将在新目录分组中显示，并在下次继续会话时使用该工作目录。历史记录中的旧目录不会被改写，修改前会自动备份会话文件。") }}
        {{ t("如果目标会话正在 Codex 中运行，请先关闭该会话再修改。") }}
      </a-alert>
      <label class="session-directory-field">
        <span>{{ t("工作目录") }}</span>
        <div class="session-directory-input">
          <a-input
            v-model="projectPath"
            :disabled="saving"
            :placeholder="t('请选择已有的工作目录')"
            @press-enter="submit"
          />
          <a-button :disabled="saving" @click="chooseDirectory">
            <template #icon><icon-folder /></template>
            {{ t("选择目录") }}
          </a-button>
        </div>
      </label>
      <div class="session-mutation-actions">
        <a-button :disabled="saving" @click="updateVisible(false)">{{ t("取消") }}</a-button>
        <a-button type="primary" :loading="saving" :disabled="!projectPath.trim()" @click="submit">
          <template #icon><icon-save /></template>
          {{ t("保存") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
