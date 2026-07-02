<script setup lang="ts">
import { t } from "../i18n";
import type { ActiveView } from "../types/ui";

defineProps<{
  activeView: ActiveView;
  accountsCount: number;
  oauthCount: number;
  apiKeyCount: number;
  currentAccountLabel: string;
  currentAccountError: string;
  detectingCurrentAccount: boolean;
  privacyMasked: boolean;
}>();

defineEmits<{
  (event: "switch-view", view: ActiveView): void;
  (event: "detect-current-account"): void;
  (event: "toggle-privacy"): void;
  (event: "open-badge-style"): void;
  (event: "open-add", tab: "oauth" | "token" | "apikey"): void;
}>();
</script>

<template>
  <header class="topbar">
    <div class="brand">
      <h1>Codex Switcher</h1>
      <p>{{ t("管理 OAuth 与 API Key 登录态，并写回本机 Codex 配置。") }}</p>
    </div>
  </header>

  <section v-if="activeView === 'accounts'" class="status-line">
    <a-tag color="arcoblue">{{ t("全部") }} {{ accountsCount }}</a-tag>
    <a-tag color="green">OAuth {{ oauthCount }}</a-tag>
    <a-tag color="orange">API Key {{ apiKeyCount }}</a-tag>
    <span v-if="currentAccountLabel">{{ t("当前：") }} {{ currentAccountLabel }}</span>
    <a-tag v-if="currentAccountError" color="red">
      {{ currentAccountError }}
    </a-tag>
  </section>

  <section class="command-bar">
    <div class="view-tabs">
      <a-button :type="activeView === 'accounts' ? 'primary' : 'secondary'" @click="$emit('switch-view', 'accounts')">
        {{ t("账号总览") }}
      </a-button>
      <a-button :type="activeView === 'sessions' ? 'primary' : 'secondary'" @click="$emit('switch-view', 'sessions')">
        <template #icon><icon-folder /></template>
        {{ t("会话管理") }}
      </a-button>
      <a-button :type="activeView === 'usage' ? 'primary' : 'secondary'" @click="$emit('switch-view', 'usage')">
        <template #icon><icon-bar-chart /></template>
        {{ t("使用统计") }}
      </a-button>
      <a-button :type="activeView === 'apiService' ? 'primary' : 'secondary'" @click="$emit('switch-view', 'apiService')">
        <template #icon><icon-code /></template>
        {{ t("API 服务") }}
      </a-button>
      <a-button :type="activeView === 'settings' ? 'primary' : 'secondary'" @click="$emit('switch-view', 'settings')">
        <template #icon><icon-settings /></template>
        {{ t("设置") }}
      </a-button>
      <a-button :type="activeView === 'about' ? 'primary' : 'secondary'" @click="$emit('switch-view', 'about')">
        <template #icon><icon-info-circle /></template>
        {{ t("关于") }}
      </a-button>
    </div>
    <div v-if="activeView === 'accounts'" class="command-actions">
      <a-button :loading="detectingCurrentAccount" @click="$emit('detect-current-account')">
        <template #icon><icon-refresh /></template>
        {{ t("读取当前账号") }}
      </a-button>
      <a-button @click="$emit('toggle-privacy')">
        <template #icon>
          <icon-eye-invisible v-if="privacyMasked" />
          <icon-eye v-else />
        </template>
        {{ privacyMasked ? t("已隐藏") : t("隐私") }}
      </a-button>
      <a-button @click="$emit('open-badge-style')">
        <template #icon><icon-palette /></template>
        {{ t("徽章样式") }}
      </a-button>
      <a-button type="primary" @click="$emit('open-add', 'oauth')">
        <template #icon><icon-plus /></template>
        {{ t("添加账号") }}
      </a-button>
    </div>
  </section>
</template>
