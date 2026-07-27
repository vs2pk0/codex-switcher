<script setup lang="ts">
import { ref } from "vue";
import appIconUrl from "../assets/app-icon.png";
import { t } from "../i18n";
import type { ActiveView } from "../types/ui";

defineProps<{
  activeView: ActiveView;
  sidebarEnabled: boolean;
  accountsCount: number;
  oauthCount: number;
  apiKeyCount: number;
  abnormalCount: number;
  currentAccountLabel: string;
  currentAccountError: string;
  detectingCurrentAccount: boolean;
  refreshingAllQuotas: boolean;
  monitorQuota: boolean;
  privacyMasked: boolean;
}>();

defineEmits<{
  (event: "switch-view", view: ActiveView): void;
  (event: "detect-current-account"): void;
  (event: "refresh-all-quotas"): void;
  (event: "toggle-privacy"): void;
  (event: "open-badge-style"): void;
  (event: "open-add", tab: "oauth" | "token" | "apikey"): void;
}>();

const sidebarCollapsed = ref(true);
</script>

<template>
  <aside v-if="sidebarEnabled" class="app-sidebar" :class="{ collapsed: sidebarCollapsed }">
    <div class="sidebar-brand">
      <img :src="appIconUrl" alt="Codex Switcher" />
      <strong>Codex Switcher</strong>
    </div>
    <nav class="sidebar-nav">
      <a-tooltip :content="t('账号总览')" position="right" :disabled="!sidebarCollapsed">
        <button
          type="button"
          :class="{ active: activeView === 'accounts' }"
          @click="$emit('switch-view', 'accounts')"
        >
          <icon-list />
          <span class="sidebar-label">{{ t("账号总览") }}</span>
        </button>
      </a-tooltip>
      <a-tooltip :content="t('会话管理')" position="right" :disabled="!sidebarCollapsed">
        <button
          type="button"
          :class="{ active: activeView === 'sessions' }"
          @click="$emit('switch-view', 'sessions')"
        >
          <icon-folder />
          <span class="sidebar-label">{{ t("会话管理") }}</span>
        </button>
      </a-tooltip>
      <a-tooltip :content="t('使用统计')" position="right" :disabled="!sidebarCollapsed">
        <button
          type="button"
          :class="{ active: activeView === 'usage' }"
          @click="$emit('switch-view', 'usage')"
        >
          <icon-bar-chart />
          <span class="sidebar-label">{{ t("使用统计") }}</span>
        </button>
      </a-tooltip>
      <a-tooltip :content="t('API 服务')" position="right" :disabled="!sidebarCollapsed">
        <button
          type="button"
          :class="{ active: activeView === 'apiService' }"
          @click="$emit('switch-view', 'apiService')"
        >
          <icon-code />
          <span class="sidebar-label">{{ t("API 服务") }}</span>
        </button>
      </a-tooltip>
      <a-tooltip :content="t('设置')" position="right" :disabled="!sidebarCollapsed">
        <button
          type="button"
          :class="{ active: activeView === 'settings' || activeView === 'pushSettings' }"
          @click="$emit('switch-view', 'settings')"
        >
          <icon-settings />
          <span class="sidebar-label">{{ t("设置") }}</span>
        </button>
      </a-tooltip>
      <a-tooltip :content="t('关于')" position="right" :disabled="!sidebarCollapsed">
        <button
          type="button"
          :class="{ active: activeView === 'about' }"
          @click="$emit('switch-view', 'about')"
        >
          <icon-info-circle />
          <span class="sidebar-label">{{ t("关于") }}</span>
        </button>
      </a-tooltip>
    </nav>
    <button
      class="sidebar-toggle"
      type="button"
      :title="sidebarCollapsed ? t('展开侧边栏') : t('收起侧边栏')"
      @click="sidebarCollapsed = !sidebarCollapsed"
    >
      <icon-right />
    </button>
  </aside>

  <header class="topbar">
    <div class="brand">
      <h1>Codex Switcher</h1>
      <p>{{ t("管理 OAuth 与 API Key 登录态，并写回本机 Codex 配置。") }}</p>
      <section v-if="activeView === 'accounts'" class="status-line">
        <a-tag color="arcoblue">{{ t("全部") }} {{ accountsCount }}</a-tag>
        <a-tag color="green">OAuth {{ oauthCount }}</a-tag>
        <a-tag color="orange">API Key {{ apiKeyCount }}</a-tag>
        <span v-if="currentAccountLabel">{{ t("当前：") }} {{ currentAccountLabel }}</span>
        <a-tag v-if="currentAccountError" color="red">
          {{ currentAccountError }}
        </a-tag>
        <a-tag v-if="abnormalCount" color="red">{{ t("异常账号") }} {{ abnormalCount }}</a-tag>
      </section>
    </div>
    <div v-if="activeView === 'accounts'" class="command-actions">
      <a-button
        class="command-button command-quota"
        :loading="refreshingAllQuotas"
        :disabled="!monitorQuota"
        @click="$emit('refresh-all-quotas')"
      >
        <template #icon><icon-refresh /></template>
        {{ t("刷新全部额度") }}
      </a-button>
      <a-button
        class="command-button command-detect"
        :loading="detectingCurrentAccount"
        @click="$emit('detect-current-account')"
      >
        <template #icon><icon-refresh /></template>
        {{ t("读取当前账号") }}
      </a-button>
      <a-button
        class="command-button command-privacy"
        :class="{ 'is-active': privacyMasked }"
        @click="$emit('toggle-privacy')"
      >
        <template #icon>
          <icon-eye-invisible v-if="privacyMasked" />
          <icon-eye v-else />
        </template>
        {{ privacyMasked ? t("已隐藏") : t("隐私") }}
      </a-button>
      <a-button class="command-button command-badge" @click="$emit('open-badge-style')">
        <template #icon><icon-palette /></template>
        {{ t("徽章样式") }}
      </a-button>
      <a-button
        type="primary"
        class="command-button command-add"
        @click="$emit('open-add', 'oauth')"
      >
        <template #icon><icon-plus /></template>
        {{ t("添加账号") }}
      </a-button>
    </div>
  </header>

  <section v-if="!sidebarEnabled" class="top-menu-bar">
    <nav class="top-view-tabs">
      <a-button :type="activeView === 'accounts' ? 'primary' : 'text'" @click="$emit('switch-view', 'accounts')">
        <template #icon><icon-list /></template>
        {{ t("账号总览") }}
      </a-button>
      <a-button :type="activeView === 'sessions' ? 'primary' : 'text'" @click="$emit('switch-view', 'sessions')">
        <template #icon><icon-folder /></template>
        {{ t("会话管理") }}
      </a-button>
      <a-button :type="activeView === 'usage' ? 'primary' : 'text'" @click="$emit('switch-view', 'usage')">
        <template #icon><icon-bar-chart /></template>
        {{ t("使用统计") }}
      </a-button>
      <a-button :type="activeView === 'apiService' ? 'primary' : 'text'" @click="$emit('switch-view', 'apiService')">
        <template #icon><icon-code /></template>
        {{ t("API 服务") }}
      </a-button>
      <a-button :type="activeView === 'settings' || activeView === 'pushSettings' ? 'primary' : 'text'" @click="$emit('switch-view', 'settings')">
        <template #icon><icon-settings /></template>
        {{ t("设置") }}
      </a-button>
      <a-button :type="activeView === 'about' ? 'primary' : 'text'" @click="$emit('switch-view', 'about')">
        <template #icon><icon-info-circle /></template>
        {{ t("关于") }}
      </a-button>
    </nav>
  </section>
</template>
