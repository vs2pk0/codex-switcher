<script setup lang="ts">
import { t } from "../i18n";
import type { CodexAccount } from "../types/codex";

defineProps<{
  visible: boolean;
  title: string;
  activeTab: string;
  editingAccount: CodexAccount | null;
  editForm: {
    accountName: string;
    tags: string[];
    apiKey: string;
    apiBaseUrl: string;
    apiProviderName: string;
    apiOfficialUrl: string;
  };
  editJsonText: string;
  editing: boolean;
  tagOptions: string[];
  isApiKeyAccount: (account: CodexAccount) => boolean;
}>();

defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "update:active-tab", value: string): void;
  (event: "update:edit-json-text", value: string): void;
  (event: "save"): void;
}>();
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t(title)"
    :footer="false"
    width="760px"
    @update:visible="$emit('update:visible', $event)"
  >
    <div class="modal-form">
      <a-tabs :active-key="activeTab" @change="$emit('update:active-tab', String($event))">
        <a-tab-pane key="form" :title="t('表单')">
          <a-form :model="editForm" layout="vertical">
            <a-form-item :label="t('账号名称')">
              <a-input v-model="editForm.accountName" :placeholder="t('例如：主力账号')" />
            </a-form-item>
            <a-form-item :label="t('标签')">
              <a-select
                v-model="editForm.tags"
                multiple
                allow-create
                allow-clear
                popup-container="body"
                :placeholder="t('选择或输入标签')"
              >
                <a-option v-for="tag in tagOptions" :key="tag" :value="tag">
                  {{ tag }}
                </a-option>
              </a-select>
            </a-form-item>
            <a-form-item v-if="editingAccount && isApiKeyAccount(editingAccount)" :label="t('供应商')">
              <a-input v-model="editForm.apiProviderName" placeholder="OpenAI Official" />
            </a-form-item>
            <a-form-item v-if="editingAccount && isApiKeyAccount(editingAccount)" :label="t('Base URL')">
              <a-input v-model="editForm.apiBaseUrl" placeholder="https://api.openai.com/v1" />
            </a-form-item>
            <a-form-item v-if="editingAccount && isApiKeyAccount(editingAccount)" :label="t('官网地址')">
              <a-input v-model="editForm.apiOfficialUrl" placeholder="https://platform.openai.com" />
            </a-form-item>
            <a-form-item v-if="editingAccount && isApiKeyAccount(editingAccount)" :label="t('API Key')">
              <a-input-password
                v-model="editForm.apiKey"
                autocomplete="new-password"
                placeholder="sk-..."
              />
            </a-form-item>
          </a-form>
        </a-tab-pane>
        <a-tab-pane key="json" :title="t('JSON')">
          <a-textarea
            :model-value="editJsonText"
            class="token-textarea json-edit-area"
            :auto-size="{ minRows: 12, maxRows: 20 }"
            @input="$emit('update:edit-json-text', String($event))"
          />
        </a-tab-pane>
      </a-tabs>
      <div class="form-actions">
        <a-button @click="$emit('update:visible', false)">{{ t("取消") }}</a-button>
        <a-button type="primary" :loading="editing" @click="$emit('save')">
          <template #icon><icon-save /></template>
          {{ t("保存") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
