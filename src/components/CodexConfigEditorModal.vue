<script setup lang="ts">
import { computed } from "vue";
import type { CodexConfigFileContent, CodexConfigFileKind } from "../services/codex";
import { t } from "../i18n";

const props = defineProps<{
  visible: boolean;
  fileKind: CodexConfigFileKind;
  file: CodexConfigFileContent | null;
  content: string;
  loading: boolean;
  saving: boolean;
  formatting: boolean;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "update:content", value: string): void;
  (event: "reload"): void;
  (event: "format"): void;
  (event: "save"): void;
}>();

const fileName = computed(() => props.file?.name || (props.fileKind === "auth" ? "auth.json" : "config.toml"));
const languageLabel = computed(() => (props.fileKind === "auth" ? "JSON" : "TOML"));
</script>

<template>
  <a-modal
    :visible="visible"
    :width="960"
    :footer="false"
    :unmount-on-close="true"
    modal-class="codex-config-editor-modal"
    @cancel="$emit('update:visible', false)"
  >
    <template #title>
      <span class="config-editor-title">
        <icon-code />
        {{ t("编辑") }} {{ fileName }}
        <em>{{ languageLabel }}</em>
      </span>
    </template>

    <a-spin :loading="loading" dot>
      <div class="config-editor-body">
        <div class="config-editor-meta">
          <div>
            <strong>{{ fileName }}</strong>
            <span>{{ file?.exists ? t("文件已存在") : t("文件不存在，保存时会创建") }}</span>
          </div>
          <code :title="file?.path">{{ file?.path || "~/.codex" }}</code>
        </div>

        <a-alert v-if="fileKind === 'auth'" type="warning" class="config-editor-warning">
          {{ t("auth.json 包含登录令牌等敏感信息，请勿截图、复制或分享给他人。") }}
        </a-alert>

        <a-textarea
          class="config-code-editor"
          :model-value="content"
          :disabled="loading || saving"
          :placeholder="t('请输入配置内容')"
          :auto-size="{ minRows: 18, maxRows: 28 }"
          @input="$emit('update:content', String($event))"
        />

        <div class="config-editor-actions">
          <div>
            <a-button :loading="formatting" :disabled="loading || saving" @click="$emit('format')">
              <template #icon><icon-brush /></template>
              {{ t("格式化并检查") }}
            </a-button>
            <a-button :disabled="saving" @click="$emit('reload')">
              <template #icon><icon-refresh /></template>
              {{ t("重新加载") }}
            </a-button>
          </div>
          <div>
            <a-button :disabled="saving" @click="$emit('update:visible', false)">
              {{ t("取消") }}
            </a-button>
            <a-button type="primary" :loading="saving" :disabled="loading" @click="$emit('save')">
              <template #icon><icon-save /></template>
              {{ t("保存文件") }}
            </a-button>
          </div>
        </div>
      </div>
    </a-spin>
  </a-modal>
</template>
