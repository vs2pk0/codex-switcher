<script setup lang="ts">
import { computed, ref } from "vue";
import { t } from "../i18n";
import type { CodexAccount } from "../types/codex";

type AddAccountTab = "oauth" | "token" | "apikey";

defineProps<{
  visible: boolean;
  title: string;
  activeTab: AddAccountTab;
  oauthUrl: string;
  oauthCallbackInput: string;
  oauthLoginId: string;
  oauthPreparing: boolean;
  oauthCompleting: boolean;
  oauthError: string;
  oauthCallbackReceived: boolean;
  tokenInput: string;
  importing: boolean;
  savingApiKey: boolean;
  apiKeyForm: {
    apiKey: string;
    apiBaseUrl: string;
    apiProviderName: string;
    apiOfficialUrl: string;
    accountName: string;
    boundOauthAccountId: string;
  };
  oauthAccounts: CodexAccount[];
  displayName: (account: CodexAccount) => string;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "update:active-tab", value: AddAccountTab): void;
  (event: "update:oauth-callback-input", value: string): void;
  (event: "update:token-input", value: string): void;
  (event: "tab-change", key: string | number): void;
  (event: "start-or-open-oauth"): void;
  (event: "copy-oauth-url"): void;
  (event: "submit-oauth-callback"): void;
  (event: "local-import"): void;
  (event: "files-import", files: File[]): void;
  (event: "token-import"): void;
  (event: "api-key-add"): void;
}>();

const fileInput = ref<HTMLInputElement | null>(null);
const tokenExamplePlaceholder = computed(
  () => `${t("示例：")}{"tokens":{"access_token":"eyJ...","refresh_token":"rt_..."}}`,
);

function openFileImport(): void {
  fileInput.value?.click();
}

function handleFileChange(event: Event): void {
  const target = event.target as HTMLInputElement;
  const files = [...(target.files ?? [])];
  target.value = "";
  if (files.length) emit("files-import", files);
}

function handleTabChange(key: string | number): void {
  emit("update:active-tab", String(key) as AddAccountTab);
  emit("tab-change", key);
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="title"
    :footer="false"
    width="820px"
    modal-class="add-account-modal"
    @update:visible="$emit('update:visible', $event)"
  >
    <div class="add-account-intro">
      <div>
        <span class="modal-eyebrow">Account Setup</span>
        <h3>{{ t("选择一种方式，把账号接到 Codex Switcher") }}</h3>
      </div>
      <p>{{ t("推荐使用浏览器授权；如果已经有本地 token、JSON 或 API Key，也可以直接导入。") }}</p>
    </div>
    <a-tabs :active-key="activeTab" class="add-account-tabs" @change="handleTabChange">
      <a-tab-pane key="oauth" :title="t('OAuth 授权')">
        <div class="oauth-connect-layout">
          <aside class="oauth-guide-card">
            <span class="modal-eyebrow">Browser Flow</span>
            <h4>{{ t("浏览器登录，自动带回授权结果") }}</h4>
            <ul>
              <li>{{ t("先生成一次性授权链接") }}</li>
              <li>{{ t("在浏览器完成 OpenAI 登录") }}</li>
              <li>{{ t("回调成功后应用会自动保存账号") }}</li>
            </ul>
            <div class="oauth-guide-note">
              {{ t("如果浏览器没有自动回到应用，可复制地址栏里的 localhost 回调地址继续。") }}
            </div>
          </aside>
          <div class="modal-form oauth-form">
            <div v-if="oauthError" class="oauth-error">{{ oauthError }}</div>
            <div v-else-if="oauthCallbackReceived" class="oauth-success">
              {{ t("回调已收到，正在写入账号；如果保存失败，可以点下方按钮重试。") }}
            </div>
            <div class="oauth-primary-action">
              <a-button
                type="primary"
                long
                size="large"
                :loading="oauthPreparing"
                @click="$emit('start-or-open-oauth')"
              >
                <template #icon><icon-globe /></template>
                {{ oauthUrl ? t("继续打开授权页") : t("生成并打开授权页") }}
              </a-button>
              <a-button v-if="oauthUrl" @click="$emit('copy-oauth-url')">
                <template #icon><icon-copy /></template>
                {{ t("复制链接") }}
              </a-button>
            </div>
            <div class="oauth-link-block compact">
              <label>{{ t("当前授权地址") }}</label>
              <a-input :model-value="oauthUrl" readonly :placeholder="t('点击上方按钮后生成授权地址')" />
            </div>
            <div class="oauth-manual-box">
              <div>
                <strong>{{ t("手动完成") }}</strong>
                <span>{{ t("浏览器未自动返回时，把 localhost 回调地址粘贴到这里。") }}</span>
              </div>
              <div class="oauth-url-row">
                <a-input
                  :model-value="oauthCallbackInput"
                  placeholder="http://localhost:1455/auth/callback?code=...&state=..."
                  @input="$emit('update:oauth-callback-input', String($event))"
                />
                <a-button
                  type="primary"
                  :loading="oauthCompleting"
                  :disabled="!oauthLoginId"
                  @click="$emit('submit-oauth-callback')"
                >
                  <template #icon><icon-check /></template>
                  {{ oauthCallbackReceived && !oauthCallbackInput.trim() ? t("重试保存") : t("完成接入") }}
                </a-button>
              </div>
            </div>
          </div>
        </div>
      </a-tab-pane>
      <a-tab-pane key="token" title="Token / JSON">
        <div class="modal-form">
          <a-typography-paragraph>
            {{ t("粘贴 session JSON、auth.json、账号 JSON、accessToken 或 refresh_token。") }}
          </a-typography-paragraph>
          <div class="local-import-actions">
            <a-button type="primary" :loading="importing" @click="$emit('local-import')">
              <template #icon><icon-folder /></template>
              {{ t("获取本地账号") }}
            </a-button>
            <a-button :loading="importing" @click="openFileImport">
              <template #icon><icon-import /></template>
              {{ t("从本地文件导入") }}
            </a-button>
            <input
              ref="fileInput"
              type="file"
              accept=".json,application/json"
              multiple
              class="hidden-file-input"
              @change="handleFileChange"
            />
          </div>
          <a-textarea
            :model-value="tokenInput"
            class="token-textarea"
            :auto-size="{ minRows: 7, maxRows: 12 }"
            :placeholder="tokenExamplePlaceholder"
            @input="$emit('update:token-input', String($event))"
          />
          <div class="form-actions">
            <a-button @click="$emit('update:visible', false)">{{ t("取消") }}</a-button>
            <a-button type="primary" :loading="importing" @click="$emit('token-import')">
              <template #icon><icon-import /></template>
              {{ t("导入") }}
            </a-button>
          </div>
        </div>
      </a-tab-pane>
      <a-tab-pane key="apikey" title="API Key">
        <div class="modal-form">
          <a-form :model="apiKeyForm" layout="vertical">
            <a-form-item :label="t('账号名称')">
              <a-input v-model="apiKeyForm.accountName" :placeholder="t('例如：本地 codex 代理')" />
            </a-form-item>
            <a-form-item :label="t('供应商')">
              <a-input v-model="apiKeyForm.apiProviderName" placeholder="OpenAI Official" />
            </a-form-item>
            <a-form-item label="Base URL">
              <a-input v-model="apiKeyForm.apiBaseUrl" placeholder="https://api.openai.com/v1" />
            </a-form-item>
            <a-form-item :label="t('官网地址')">
              <a-input v-model="apiKeyForm.apiOfficialUrl" placeholder="https://platform.openai.com" />
            </a-form-item>
            <a-form-item label="API Key">
              <a-input-password
                v-model="apiKeyForm.apiKey"
                autocomplete="new-password"
                placeholder="sk-..."
              />
            </a-form-item>
            <a-form-item v-if="oauthAccounts.length" :label="t('绑定已有 OAuth 账号')">
              <a-select
                v-model="apiKeyForm.boundOauthAccountId"
                allow-clear
                :placeholder="t('可选：用于保留 Codex 会话身份')"
              >
                <a-option
                  v-for="oauth in oauthAccounts"
                  :key="oauth.id"
                  :value="oauth.id"
                >
                  {{ displayName(oauth) }}
                </a-option>
              </a-select>
            </a-form-item>
          </a-form>
          <div class="form-actions">
            <a-button @click="$emit('update:visible', false)">{{ t("取消") }}</a-button>
            <a-button type="primary" :loading="savingApiKey" @click="$emit('api-key-add')">
              <template #icon><icon-plus /></template>
              {{ t("添加") }}
            </a-button>
          </div>
        </div>
      </a-tab-pane>
    </a-tabs>
  </a-modal>
</template>
