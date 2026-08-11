<script setup lang="ts">
import { ref, watch } from "vue";
import { Message } from "@arco-design/web-vue";
import { t } from "../i18n";
import type { CodexSessionRecord } from "../services/session";

const props = defineProps<{
  visible: boolean;
  session: CodexSessionRecord | null;
  saving: boolean;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "save", title: string): void;
}>();

const title = ref("");

watch(
  () => [props.visible, props.session?.id],
  ([visible]) => {
    if (visible) title.value = props.session?.title || "";
  },
);

function updateVisible(visible: boolean): void {
  if (!props.saving) emit("update:visible", visible);
}

function submit(): void {
  const nextTitle = title.value.trim();
  if (!nextTitle) {
    Message.warning(t("会话名称不能为空"));
    return;
  }
  emit("save", nextTitle);
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('修改会话名称')"
    :footer="false"
    :closable="!saving"
    :mask-closable="!saving"
    width="480px"
    modal-class="session-rename-modal"
    @update:visible="updateVisible"
  >
    <div class="session-rename-modal-body">
      <label>
        <span>{{ t("会话名称") }}</span>
        <a-input
          v-model="title"
          :disabled="saving"
          :max-length="100"
          show-word-limit
          autofocus
          :placeholder="t('输入新的会话名称')"
          @press-enter="submit"
        />
      </label>
      <div class="session-mutation-actions">
        <a-button :disabled="saving" @click="updateVisible(false)">{{ t("取消") }}</a-button>
        <a-button type="primary" :loading="saving" :disabled="!title.trim()" @click="submit">
          <template #icon><icon-save /></template>
          {{ t("保存") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
