<script setup lang="ts">
import { computed } from "vue";
import type { CodexSwitcherSettings } from "../services/codex";
import { currentLanguage, t } from "../i18n";

const props = defineProps<{
  settings: CodexSwitcherSettings;
  isCurrentPageSelected: boolean;
  accountTypeOptions: Array<{ label: string; value: string }>;
  accountSearchKeyword: string;
  showSortDirection: boolean;
  currentAccountRefreshCountdown: string;
  quotaRefreshCountdown: string;
}>();

const emit = defineEmits<{
  (event: "toggle-all", checked: boolean): void;
  (event: "update:account-search-keyword", value: string): void;
  (event: "reset-page"): void;
  (event: "save-settings"): void;
  (event: "open-sort-editor"): void;
  (event: "bind-selected", target: "api-service" | "open-codex"): void;
  (event: "batch-export"): void;
  (event: "open-add", tab: "oauth" | "token" | "apikey"): void;
}>();

type AccountViewMode = CodexSwitcherSettings["accountViewMode"];

const accountViewModes: AccountViewMode[] = ["card", "compact", "table"];
const accountViewModeLabels: Record<AccountViewMode, string> = {
  card: "卡片视图",
  compact: "紧凑视图",
  table: "表格视图",
};
const usesLongCopyLayout = computed(() =>
  currentLanguage.value === "en" || currentLanguage.value === "ru",
);

function accountViewModeLabel(): string {
  return t(accountViewModeLabels[props.settings.accountViewMode] ?? accountViewModeLabels.card);
}

function cycleAccountViewMode(): void {
  const currentIndex = accountViewModes.indexOf(props.settings.accountViewMode);
  const nextIndex = currentIndex >= 0 ? (currentIndex + 1) % accountViewModes.length : 0;
  props.settings.accountViewMode = accountViewModes[nextIndex];
  emit("save-settings");
}

function handleSortModeChange(value: string): void {
  if (value === "quota_reset_countdown" || value === "tags") {
    props.settings.sortDirection = "asc";
  }
  emit("save-settings");
}

function handleBindTarget(value: string | number | Record<string, unknown> | undefined): void {
  if (value === "api-service" || value === "open-codex") {
    emit("bind-selected", value);
  }
}
</script>

<template>
  <section class="account-ops" :class="{ 'account-ops-long-copy': usesLongCopyLayout }">
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
        :trigger-props="{
          contentClass: 'account-filter-dropdown account-type-dropdown',
          autoFitPopupWidth: false,
          autoFitPopupMinWidth: true,
        }"
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
        :placeholder="t('筛选邮箱 / 昵称 / 标签')"
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
        @change="(value) => handleSortModeChange(String(value))"
      >
        <a-option value="created_at">{{ t("按创建时间") }}</a-option>
        <a-option value="weekly_quota">{{ t("按周配额") }}</a-option>
        <a-option value="hourly_quota">{{ t("按5小时配额") }}</a-option>
        <a-option value="quota_reset_countdown">{{ t("按额度恢复倒计时") }}</a-option>
        <a-option value="tags">{{ t("按标签") }}</a-option>
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
      <a-tooltip :content="`${t('切换视图')}：${accountViewModeLabel()}`">
        <a-button
          class="account-view-icon-button"
          :title="`${t('切换视图')}：${accountViewModeLabel()}`"
          @click="cycleAccountViewMode"
        >
          <template #icon><icon-list /></template>
        </a-button>
      </a-tooltip>
    </div>
    <div class="account-ops-footer">
      <div class="account-batch-actions">
        <a-dropdown
          trigger="click"
          position="bl"
          popup-container="body"
          :popup-max-height="false"
          @select="handleBindTarget"
        >
          <a-button class="batch-action batch-bind">
            <template #icon><icon-link /></template>
            {{ t("绑定") }} <icon-down />
          </a-button>
          <template #content>
            <a-doption value="api-service">{{ t("绑定到 API 服务") }}</a-doption>
            <a-doption value="open-codex">{{ t("绑定到 OpenCodex") }}</a-doption>
          </template>
        </a-dropdown>
        <a-button class="batch-action batch-export" @click="$emit('batch-export')">
          <template #icon><icon-download /></template>
          {{ t("批量导出") }}
        </a-button>
        <a-button class="batch-action batch-import" @click="$emit('open-add', 'token')">
          <template #icon><icon-import /></template>
          {{ t("批量导入") }}
        </a-button>
      </div>
      <div
        v-if="settings.monitorQuota || currentAccountRefreshCountdown || quotaRefreshCountdown"
        class="quota-countdown-group"
      >
        <span v-if="currentAccountRefreshCountdown" class="quota-countdown primary">
          {{ t("当前账号") }} {{ currentAccountRefreshCountdown }}
        </span>
        <span v-if="quotaRefreshCountdown" class="quota-countdown">
          {{ t("当前页") }} {{ quotaRefreshCountdown }}
        </span>
      </div>
    </div>
  </section>
</template>
