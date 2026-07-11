<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { CodexApiKeyModel } from "../services/codex";
import type { CodexAccount } from "../types/codex";
import { t } from "../i18n";

const props = defineProps<{
  visible: boolean;
  account: CodexAccount | null;
  models: CodexApiKeyModel[];
  selectedModel: string;
  loading: boolean;
  saving: boolean;
  accessStatus: "idle" | "checking" | "matched" | "mismatched" | "error";
  accessError: string;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "update:selected-model", value: string): void;
  (event: "check-access"): void;
  (event: "fetch"): void;
  (event: "save"): void;
}>();

const searchQuery = ref("");

const currentDefaultModel = computed(
  () => props.account?.default_model || props.account?.defaultModel || "",
);
const canSetModel = computed(() => props.accessStatus === "matched");

const filteredModels = computed(() => {
  const query = searchQuery.value.trim().toLocaleLowerCase();
  if (!query) return props.models;
  return props.models.filter((model) =>
    [model.id, model.ownedBy || ""].some((value) => value.toLocaleLowerCase().includes(query)),
  );
});

const selectedIsGpt56 = computed(() => isGpt56Model(props.selectedModel));

function isGpt56Model(modelId: string): boolean {
  return /^gpt-5\.6(?:-|$)/i.test(modelId.trim());
}

watch(
  () => props.visible,
  (visible) => {
    if (visible) searchQuery.value = "";
  },
);
</script>

<template>
  <a-modal
    :visible="visible"
    :footer="false"
    :closable="!saving"
    :mask-closable="!saving"
    :esc-to-close="!saving"
    width="760px"
    modal-class="api-key-model-modal"
    @update:visible="$emit('update:visible', $event)"
  >
    <template #title>
      <span class="api-model-modal-title">
        <icon-list />
        {{ t("API Key 模型列表") }}
      </span>
    </template>

    <div v-if="account" class="api-model-modal-body">
      <div class="api-model-account-summary">
        <div>
          <span>{{ t("目标账号") }}</span>
          <strong>{{ account.account_name || account.api_provider_name || account.email }}</strong>
        </div>
        <div>
          <span>{{ t("Base URL") }}</span>
          <code :title="account.api_base_url || account.apiBaseUrl || 'https://api.openai.com/v1'">
            {{ account.api_base_url || account.apiBaseUrl || "https://api.openai.com/v1" }}
          </code>
        </div>
        <a-tag v-if="currentDefaultModel" color="arcoblue">
          {{ t("当前默认") }} · {{ currentDefaultModel }}
        </a-tag>
      </div>

      <div v-if="accessStatus !== 'idle'" class="api-model-access-row">
        <a-alert
          v-if="accessStatus === 'checking'"
          type="info"
          show-icon
          class="api-model-access-alert"
        >
          {{ t("正在读取本机 Codex 配置并核对当前 API Key…") }}
        </a-alert>
        <a-alert
          v-else-if="accessStatus === 'matched'"
          type="success"
          show-icon
          class="api-model-access-alert"
        >
          {{ t("当前 API Key 与目标账号匹配，可以设置模型。") }}
        </a-alert>
        <a-alert
          v-else-if="accessStatus === 'mismatched'"
          type="warning"
          show-icon
          class="api-model-access-alert"
        >
          {{ t("当前 Codex 配置不是此 API Key，请先切换到该账号后再设置模型。") }}
        </a-alert>
        <a-alert v-else type="error" show-icon class="api-model-access-alert">
          {{ t("读取本机 Codex 配置失败") }}：{{ accessError }}
        </a-alert>
        <a-button
          v-if="accessStatus === 'mismatched' || accessStatus === 'error'"
          class="api-model-access-retry"
          :disabled="loading || saving"
          @click="$emit('check-access')"
        >
          <template #icon><icon-refresh /></template>
          {{ t("重新检测") }}
        </a-button>
      </div>

      <div class="api-model-toolbar">
        <a-input
          v-model="searchQuery"
          allow-clear
          :disabled="!models.length"
          :placeholder="t('筛选模型名称')"
        >
          <template #prefix><icon-search /></template>
        </a-input>
        <span v-if="models.length">{{ filteredModels.length }} / {{ models.length }}</span>
        <a-button
          type="primary"
          :loading="loading"
          :disabled="saving || accessStatus === 'checking'"
          @click="$emit('fetch')"
        >
          <template #icon><icon-refresh /></template>
          {{ models.length ? t("重新获取列表") : t("获取模型列表") }}
        </a-button>
      </div>

      <a-alert
        v-if="canSetModel && selectedIsGpt56"
        type="warning"
        class="api-model-compatibility-alert"
      >
        {{ t("选择 5.6 系列模型后，将同步写入 Responses 模式与兼容配置，并移除旧 WebSocket 配置。") }}
      </a-alert>

      <a-spin :loading="loading" dot>
        <div v-if="filteredModels.length" class="api-model-list">
          <button
            v-for="model in filteredModels"
            :key="model.id"
            type="button"
            class="api-model-option"
            :class="{ selected: selectedModel === model.id }"
            :disabled="!canSetModel || saving"
            @click="$emit('update:selected-model', model.id)"
          >
            <span class="api-model-option-check">
              <icon-check v-if="selectedModel === model.id" />
            </span>
            <span class="api-model-option-main">
              <strong>{{ model.id }}</strong>
              <small v-if="model.ownedBy">{{ t("提供方") }}：{{ model.ownedBy }}</small>
            </span>
            <a-tag v-if="currentDefaultModel === model.id" color="arcoblue">{{ t("当前默认") }}</a-tag>
            <a-tag v-if="isGpt56Model(model.id)" color="orange">{{ t("5.6 配置") }}</a-tag>
          </button>
        </div>
        <a-empty
          v-else
          class="api-model-empty"
          :description="models.length ? t('没有匹配的模型') : t('点击“获取模型列表”从当前 API 服务读取可用模型')"
        />
      </a-spin>

      <div class="api-model-modal-actions">
        <a-button :disabled="saving" @click="$emit('update:visible', false)">{{ t("取消") }}</a-button>
        <a-button
          type="primary"
          :loading="saving"
          :disabled="loading || !canSetModel || !selectedModel"
          @click="$emit('save')"
        >
          <template #icon><icon-save /></template>
          {{ t("设为默认模型") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
