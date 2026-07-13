<script setup lang="ts">
import { t } from "../i18n";
import { hasAnyQuotaWindow, hasQuotaWindow } from "../quota";
import type { CodexAccount } from "../types/codex";
import PlanBadge from "./PlanBadge.vue";

defineProps<{
  visible: boolean;
  bindingForm: { boundOauthAccountId: string };
  saving: boolean;
  oauthAccounts: CodexAccount[];
  displayName: (account: CodexAccount) => string;
  isFreePlanAccount: (account: CodexAccount) => boolean;
  quotaColor: (percentage: number) => string;
  quotaWindowLabel: (minutes?: number, fallback?: string) => string;
  quotaResetLabel: (timestamp?: number) => string;
  planLabel: (account: CodexAccount) => string;
  planClass: (account: CodexAccount) => string;
}>();

defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "save"): void;
}>();
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('绑定 OAuth 账号')"
    :footer="false"
    width="840px"
    @update:visible="$emit('update:visible', $event)"
  >
    <div class="modal-form">
      <a-typography-paragraph>
        {{ t("API Key 账号绑定 OAuth 后，切换时会同时写入 OAuth Token 与 API Key 配置，便于修复会话身份。") }}
      </a-typography-paragraph>
      <div class="oauth-bind-list">
        <button
          type="button"
          class="oauth-bind-card unlink"
          :class="{ selected: !bindingForm.boundOauthAccountId }"
          @click="bindingForm.boundOauthAccountId = ''"
        >
          <span class="oauth-bind-check">
            <icon-check v-if="!bindingForm.boundOauthAccountId" />
          </span>
          <div class="oauth-bind-option-title">
            <strong>{{ t("不绑定 OAuth") }}</strong>
            <span>{{ t("切换时仅写入 API Key 配置") }}</span>
          </div>
        </button>

        <button
          v-for="oauth in oauthAccounts"
          :key="oauth.id"
          type="button"
          class="oauth-bind-card"
          :class="{ selected: bindingForm.boundOauthAccountId === oauth.id }"
          @click="bindingForm.boundOauthAccountId = oauth.id"
        >
          <span class="oauth-bind-check">
            <icon-check v-if="bindingForm.boundOauthAccountId === oauth.id" />
          </span>
          <div class="oauth-bind-option">
            <div class="oauth-bind-option-head">
              <div class="oauth-bind-option-title">
                <strong>{{ displayName(oauth) }}</strong>
                <span>OAuth · {{ oauth.email || oauth.id }}</span>
              </div>
              <PlanBadge :label="planLabel(oauth)" :badge-class="planClass(oauth)" />
            </div>
            <div v-if="oauth.quota && hasAnyQuotaWindow(oauth.quota)" class="oauth-bind-quota">
              <div v-if="hasQuotaWindow(oauth.quota, 'hourly')">
                <span>
                  <icon-calendar v-if="isFreePlanAccount(oauth)" />
                  <icon-clock-circle v-else />
                  {{ isFreePlanAccount(oauth) ? t("长周期") : t("短周期") }}
                </span>
                <strong :style="{ color: quotaColor(oauth.quota.hourly_percentage) }">
                  {{ oauth.quota.hourly_percentage }}%
                </strong>
                <small>{{ quotaWindowLabel(oauth.quota.hourly_window_minutes, "5 小时窗口") }}</small>
                <em>{{ quotaResetLabel(oauth.quota.hourly_reset_time) }}</em>
              </div>
              <div
                v-if="!isFreePlanAccount(oauth) && hasQuotaWindow(oauth.quota, 'weekly')"
              >
                <span><icon-calendar /> {{ t("长周期") }}</span>
                <strong :style="{ color: quotaColor(oauth.quota.weekly_percentage) }">
                  {{ oauth.quota.weekly_percentage }}%
                </strong>
                <small>{{ quotaWindowLabel(oauth.quota.weekly_window_minutes, "7 天窗口") }}</small>
                <em>{{ quotaResetLabel(oauth.quota.weekly_reset_time) }}</em>
              </div>
            </div>
            <div v-else-if="oauth.quota_error" class="oauth-bind-quota-error">
              {{ oauth.quota_error.message }}
            </div>
          </div>
        </button>
        <a-empty v-if="!oauthAccounts.length" :description="t('暂无可绑定的 OAuth 账号')" />
      </div>
      <div class="form-actions">
        <a-button @click="$emit('update:visible', false)">{{ t("取消") }}</a-button>
        <a-button type="primary" :loading="saving" @click="$emit('save')">
          <template #icon><icon-save /></template>
          {{ t("保存") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
