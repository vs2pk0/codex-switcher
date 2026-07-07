<script setup lang="ts">
import type { CodexSwitcherSettings } from "../services/codex";
import { t } from "../i18n";

defineProps<{
  settings: CodexSwitcherSettings;
  isCurrentPageSelected: boolean;
  accountTypeOptions: Array<{ label: string; value: string }>;
  accountSearchKeyword: string;
  showSortDirection: boolean;
  currentAccountRefreshCountdown: string;
  quotaRefreshCountdown: string;
  refreshingAllQuotas: boolean;
}>();

defineEmits<{
  (event: "toggle-all", checked: boolean): void;
  (event: "update:account-search-keyword", value: string): void;
  (event: "reset-page"): void;
  (event: "save-settings"): void;
  (event: "open-sort-editor"): void;
  (event: "bind-selected-to-api-service"): void;
  (event: "batch-export"): void;
  (event: "refresh-all-quotas"): void;
  (event: "open-add", tab: "oauth" | "token" | "apikey"): void;
}>();
</script>

<template>
  <section class="account-ops">
    <div class="account-ops-left">
      <a-checkbox
        :model-value="isCurrentPageSelected"
        @change="$emit('toggle-all', Boolean($event))"
      >
        {{ t("全选") }}
      </a-checkbox>
      <a-select
        v-model="settings.accountTypeFilter"
        class="filter-select"
        popup-container="body"
        :scrollbar="false"
        :trigger-props="{ contentClass: 'account-filter-dropdown' }"
        @change="() => { $emit('reset-page'); $emit('save-settings'); }"
      >
        <a-option
          v-for="option in accountTypeOptions"
          :key="option.value"
          :value="option.value"
        >
          {{ option.label }}
        </a-option>
      </a-select>
      <a-input
        :model-value="accountSearchKeyword"
        class="account-search-input"
        allow-clear
        :placeholder="t('筛选邮箱 / 昵称')"
        @input="(value) => { $emit('update:account-search-keyword', String(value)); $emit('reset-page'); }"
        @clear="() => { $emit('update:account-search-keyword', ''); $emit('reset-page'); }"
      >
        <template #prefix><icon-search /></template>
      </a-input>
      <a-select
        v-model="settings.sortMode"
        class="sort-select"
        popup-container="body"
        :scrollbar="false"
        :trigger-props="{ contentClass: 'account-filter-dropdown account-sort-dropdown' }"
        @change="$emit('save-settings')"
      >
        <a-option value="created_at">{{ t("按创建时间") }}</a-option>
        <a-option value="weekly_quota">{{ t("按周配额") }}</a-option>
        <a-option value="hourly_quota">{{ t("按5小时配额") }}</a-option>
        <a-option value="weekly_reset">{{ t("按周配额重置时间") }}</a-option>
        <a-option value="hourly_reset">{{ t("按5小时配额重置时间") }}</a-option>
        <a-option value="subscription">{{ t("按订阅有效期") }}</a-option>
        <a-option value="custom">{{ t("自定义顺序") }}</a-option>
      </a-select>
      <a-button v-if="settings.sortMode === 'custom'" @click="$emit('open-sort-editor')">
        <template #icon><icon-list /></template>
        {{ t("编辑排序") }}
      </a-button>
      <a-radio-group
        v-if="showSortDirection"
        v-model="settings.sortDirection"
        type="button"
        size="small"
        @change="$emit('save-settings')"
      >
        <a-radio value="desc">{{ t("倒序") }}</a-radio>
        <a-radio value="asc">{{ t("正序") }}</a-radio>
      </a-radio-group>
      <a-select
        v-model="settings.pageSize"
        class="page-size-select"
        @change="() => { $emit('reset-page'); $emit('save-settings'); }"
      >
        <a-option :value="20">{{ t("每页") }} 20</a-option>
        <a-option :value="50">{{ t("每页") }} 50</a-option>
        <a-option :value="100">{{ t("每页") }} 100</a-option>
        <a-option :value="200">{{ t("每页") }} 200</a-option>
      </a-select>
      <a-button @click="$emit('bind-selected-to-api-service')">
        <template #icon><icon-link /></template>
        {{ t("绑定到 API 服务") }}
      </a-button>
      <a-button @click="$emit('batch-export')">
        <template #icon><icon-download /></template>
        {{ t("批量导出") }}
      </a-button>
      <a-button @click="$emit('open-add', 'token')">
        <template #icon><icon-import /></template>
        {{ t("批量导入") }}
      </a-button>
    </div>
    <div
      v-if="settings.monitorQuota || currentAccountRefreshCountdown || quotaRefreshCountdown"
      class="quota-countdown-group"
    >
      <a-button
        size="small"
        :loading="refreshingAllQuotas"
        :disabled="!settings.monitorQuota"
        @click="$emit('refresh-all-quotas')"
      >
        <template #icon><icon-refresh /></template>
        {{ t("刷新全部额度") }}
      </a-button>
      <span v-if="currentAccountRefreshCountdown" class="quota-countdown primary">
        {{ t("当前账号") }} {{ currentAccountRefreshCountdown }}
      </span>
      <span v-if="quotaRefreshCountdown" class="quota-countdown">
        {{ t("当前页") }} {{ quotaRefreshCountdown }}
      </span>
    </div>
  </section>
</template>
