<script setup lang="ts">
import { computed, ref } from "vue";
import type { CodexSwitcherSettings } from "../services/codex";
import type { CodexAccount, CodexResetCredit } from "../types/codex";
import PlanBadge from "./PlanBadge.vue";

const props = defineProps<{
  accounts: CodexAccount[];
  currentId: string;
  selectedAccountIds: Set<string>;
  settings: CodexSwitcherSettings;
  expandedLayout: boolean;
  loading: boolean;
  switchingId: string;
  deletingId: string;
  exportingId: string;
  quotaRefreshingId: string;
  privacyMasked: boolean;
}>();

const emit = defineEmits<{
  (event: "toggle-account", id: string): void;
  (event: "toggle-pin", account: CodexAccount): void;
  (event: "drag-start", account: CodexAccount): void;
  (event: "drag-end"): void;
  (event: "drop-account", account: CodexAccount): void;
  (event: "open-phone", account: CodexAccount): void;
  (event: "reset-credit", account: CodexAccount): void;
  (event: "open-binding", account: CodexAccount): void;
  (event: "open-official-url", url: string): void;
  (event: "open-edit", account: CodexAccount): void;
  (event: "switch-account", account: CodexAccount): void;
  (event: "refresh-quota", account: CodexAccount): void;
  (event: "open-export", account: CodexAccount): void;
  (event: "confirm-delete", account: CodexAccount): void;
  (event: "open-add", tab: string): void;
  (event: "copy-email", account: CodexAccount): void;
  (event: "reauthorize", account: CodexAccount): void;
}>();

const dragOverAccountId = ref("");

const gridClass = computed(() => {
  const columns = [3, 4, 5].includes(props.settings.maxColumns) ? props.settings.maxColumns : 3;
  return {
    [`columns-${columns}`]: true,
    expanded: props.expandedLayout,
  };
});

function isPinned(account: CodexAccount): boolean {
  return (props.settings.pinnedAccountIds || []).includes(account.id);
}

function handleDragStart(event: DragEvent, account: CodexAccount): void {
  if (props.settings.sortMode !== "custom") return;
  event.dataTransfer?.setData("text/plain", account.id);
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  emit("drag-start", account);
}

function handleDragOver(event: DragEvent, account: CodexAccount): void {
  if (props.settings.sortMode !== "custom") return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  dragOverAccountId.value = account.id;
}

function handleDrop(event: DragEvent, account: CodexAccount): void {
  if (props.settings.sortMode !== "custom") return;
  event.preventDefault();
  event.stopPropagation();
  dragOverAccountId.value = "";
  emit("drop-account", account);
}

function handleDragEnd(): void {
  dragOverAccountId.value = "";
  emit("drag-end");
}

function isApiKeyAccount(account: CodexAccount): boolean {
  return account.auth_mode === "apikey" || Boolean(account.openai_api_key || account.openaiApiKey);
}

function displayName(account: CodexAccount): string {
  return account.account_name || account.email || account.id || "未命名账号";
}

function maskEmail(value: string): string {
  const [name, domain] = value.split("@");
  if (!domain) return maskMiddle(value);
  if (name.length <= 3) return `${name[0] ?? "*"}***@${domain}`;
  return `${name.slice(0, 2)}***${name.slice(-1)}@${domain}`;
}

function maskPhone(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length <= 7) return maskMiddle(trimmed);
  return `${trimmed.slice(0, 3)}***${trimmed.slice(-4)}`;
}

function maskMiddle(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length <= 3) return trimmed ? `${trimmed[0]}***` : "";
  return `${trimmed.slice(0, 2)}***${trimmed.slice(-2)}`;
}

function displayIdentity(value: string): string {
  if (!props.privacyMasked) return value;
  if (value.includes("@")) return maskEmail(value);
  return maskMiddle(value);
}

function displayAccountName(account: CodexAccount): string {
  const raw = displayName(account);
  return displayIdentity(raw);
}

function boundOAuthAccount(account: CodexAccount): CodexAccount | undefined {
  if (!account.bound_oauth_account_id) return undefined;
  return props.accounts.find((item) => item.id === account.bound_oauth_account_id);
}

function boundOAuthName(account: CodexAccount): string {
  const bound = boundOAuthAccount(account);
  return bound ? displayAccountName(bound) : "未绑定";
}

function isBoundApiKeyAccount(account: CodexAccount): boolean {
  return isApiKeyAccount(account) && Boolean(account.bound_oauth_account_id);
}

function canShowQuota(account: CodexAccount): boolean {
  if (!props.settings.monitorQuota) return false;
  return !isApiKeyAccount(account);
}

function shouldShowQuota(account: CodexAccount): boolean {
  return canShowQuota(account) && Boolean(account.quota);
}

function shouldShowQuotaError(account: CodexAccount): boolean {
  return canShowQuota(account) && Boolean(account.quota_error);
}

function scalarText(value: unknown): string | undefined {
  if (typeof value === "string") return value.trim() || undefined;
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  return undefined;
}

function jwtPayload(token: string | undefined): Record<string, unknown> | undefined {
  const payload = token?.split(".")[1];
  if (!payload) return undefined;
  try {
    const normalized = payload.replace(/-/g, "+").replace(/_/g, "/");
    const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
    return JSON.parse(window.atob(padded)) as Record<string, unknown>;
  } catch {
    return undefined;
  }
}

function jwtClaim(token: string | undefined, claim: string): string | undefined {
  return scalarText(jwtPayload(token)?.[claim]);
}

function jwtAuthClaim(token: string | undefined, claim: string): string | undefined {
  const auth = jwtPayload(token)?.["https://api.openai.com/auth"];
  if (!auth || typeof auth !== "object" || Array.isArray(auth)) return undefined;
  return scalarText((auth as Record<string, unknown>)[claim]);
}

function accountStatusSource(account: CodexAccount): CodexAccount {
  if (isBoundApiKeyAccount(account)) return account;
  return account.subscription_active_until ||
    account.access_token_expires_at ||
    jwtAuthClaim(account.tokens.id_token, "chatgpt_subscription_active_until") ||
    jwtClaim(account.tokens.access_token, "exp") ||
    jwtClaim(account.tokens.id_token, "exp")
    ? account
    : boundOAuthAccount(account) ?? account;
}

function accountSubscriptionUntil(account: CodexAccount): string | undefined {
  const source = accountStatusSource(account);
  return (
    source.subscription_active_until ||
    jwtAuthClaim(source.tokens.id_token, "chatgpt_subscription_active_until")
  );
}

function accountTokenExpiresAt(account: CodexAccount): string | undefined {
  const source = accountStatusSource(account);
  return (
    source.access_token_expires_at ||
    jwtClaim(source.tokens.access_token, "exp") ||
    jwtClaim(source.tokens.id_token, "exp")
  );
}

function shortAccountId(account: CodexAccount): string {
  const value = account.id || account.email || displayName(account);
  if (value.length <= 8) return value;
  return `${value.slice(0, 3)}****${value.slice(-3)}`;
}

function normalizePlanKey(planType?: string): string {
  const normalized = (planType || "").trim().toLowerCase();
  if (!normalized) return "free";
  if (normalized.includes("api")) return "api_key";
  if (normalized.includes("enterprise")) return "enterprise";
  if (normalized.includes("business")) return "business";
  if (normalized.includes("team")) return "team";
  if (normalized.includes("edu")) return "edu";
  if (normalized.includes("go")) return "go";
  if (normalized.includes("plus")) return "plus";
  if (normalized.includes("pro")) return "pro";
  if (normalized.includes("free")) return "free";
  return normalized;
}

function normalizeAuthFilePlan(value?: string): "prolite" | "promax" | undefined {
  const normalized = (value || "").trim().toLowerCase().replace(/[_\s]+/g, "-");
  if (["prolite", "pro-lite", "pro-5x", "codex-pro-5x"].includes(normalized)) return "prolite";
  if (["promax", "pro-max", "pro-20x", "codex-pro-20x"].includes(normalized)) return "promax";
  return undefined;
}

function planDisplayName(planType?: string): string {
  const key = normalizePlanKey(planType);
  if (key === "enterprise") return "ENTERPRISE";
  if (key === "business") return "BUSINESS";
  if (key === "team") return "TEAM";
  if (key === "edu") return "EDU";
  if (key === "go") return "GO";
  if (key === "plus") return "PLUS";
  if (key === "pro") return "PRO";
  if (key === "api_key") return "API_KEY";
  if (key === "free") return "FREE";
  return (planType || "FREE").trim().toUpperCase();
}

function planLabel(account: CodexAccount): string {
  if (isApiKeyAccount(account)) return "API_KEY";
  const base = planDisplayName(account.plan_type);
  if (base !== "PRO") return base;
  const authPlan = normalizeAuthFilePlan(account.auth_file_plan_type || account.plan_type);
  return authPlan === "prolite" ? "PRO 5X" : "PRO 20X";
}

function planClass(account: CodexAccount): string {
  const badgeKey = badgeTypeKey(account);
  const styleName =
    props.settings.badgeStyles?.[badgeKey] ||
    props.settings.badgeStyle ||
    "classic";
  const style = `badge-${styleName}`;
  if (isApiKeyAccount(account)) return `api ${style}`;
  const key = normalizePlanKey(account.plan_type);
  if (key === "pro") {
    const proClass =
      normalizeAuthFilePlan(account.auth_file_plan_type || account.plan_type) === "prolite"
        ? "pro-lite"
        : "pro-max";
    return `${proClass} ${style}`;
  }
  return `${key} ${style}`;
}

function badgeTypeKey(account: CodexAccount): string {
  if (isApiKeyAccount(account)) return "api";
  const key = normalizePlanKey(account.plan_type);
  if (key === "pro") {
    return normalizeAuthFilePlan(account.auth_file_plan_type || account.plan_type) === "prolite"
      ? "proLite"
      : "proMax";
  }
  if (key === "plus") return "plus";
  if (["team", "business", "enterprise", "edu", "go"].includes(key)) return "team";
  return "free";
}

function accountLoginLine(account: CodexAccount): string {
  return `API Key: ${maskSecret(account.openai_api_key || account.openaiApiKey)}`;
}

function isFreePlanAccount(account: CodexAccount): boolean {
  return !isApiKeyAccount(account) && normalizePlanKey(account.plan_type) === "free";
}

function displayPhone(value: string): string {
  return props.privacyMasked ? maskPhone(value) : value;
}

function apiBaseUrl(account: CodexAccount): string {
  return (account.api_base_url || account.apiBaseUrl)?.trim() ?? "";
}

function apiBaseUrlLine(account: CodexAccount): string {
  const value = apiBaseUrl(account);
  return value ? `Base URL: ${value}` : "Base URL: 未设置";
}

function apiOfficialUrl(account: CodexAccount): string {
  return (account.api_official_url || account.apiOfficialUrl)?.trim() ?? "";
}

function maskSecret(value?: string): string {
  const trimmed = value?.trim() ?? "";
  if (!trimmed) return "未保存";
  if (trimmed.length <= 10) return `${trimmed.slice(0, 3)}****`;
  return `${trimmed.slice(0, 6)}****${trimmed.slice(-4)}`;
}

function canUseResetCredit(account: CodexAccount): boolean {
  return shouldShowQuota(account) && resetCreditCount(account) > 0;
}

function resetCreditCount(account: CodexAccount): number {
  const count = account.quota?.reset_credits_available;
  if (Number.isFinite(count)) return Math.max(0, Number(count));
  return resetCreditRecords(account).filter((credit) => resetCreditStatusKey(credit) === "available").length;
}

function resetCreditRecords(account: CodexAccount): CodexResetCredit[] {
  if (Array.isArray(account.quota?.reset_credits) && account.quota.reset_credits.length > 0) {
    return account.quota.reset_credits;
  }
  return parseResetCreditRecordsFromRawData(account.quota?.raw_data);
}

function hasResetCreditRecords(account: CodexAccount): boolean {
  return resetCreditRecords(account).length > 0;
}

function parseResetCreditRecordsFromRawData(rawData: unknown): CodexResetCredit[] {
  const root = isRecord(rawData) ? rawData : undefined;
  const container = isRecord(root?.rate_limit_reset_credits)
    ? root.rate_limit_reset_credits
    : isRecord(root?.data) && isRecord(root.data.rate_limit_reset_credits)
      ? root.data.rate_limit_reset_credits
      : undefined;
  const credits = Array.isArray(container?.credits)
    ? container.credits
    : isRecord(container?.data) && Array.isArray(container.data.credits)
      ? container.data.credits
      : [];

  return credits.filter(isRecord).map((credit) => ({
    id: normalizeScalar(credit.id) || normalizeScalar(credit.credit_id) || normalizeScalar(credit.creditId),
    status: normalizeScalar(credit.status) || normalizeScalar(credit.state),
    reset_type: normalizeScalar(credit.type) || normalizeScalar(credit.reset_type) || normalizeScalar(credit.resetType),
    granted_at:
      normalizeTimestamp(credit.granted_at) ??
      normalizeTimestamp(credit.created_at) ??
      normalizeTimestamp(credit.grantedAt),
    expires_at:
      normalizeTimestamp(credit.expires_at) ??
      normalizeTimestamp(credit.expire_at) ??
      normalizeTimestamp(credit.expiresAt),
    redeemed_at:
      normalizeTimestamp(credit.redeemed_at) ??
      normalizeTimestamp(credit.used_at) ??
      normalizeTimestamp(credit.consumed_at) ??
      normalizeTimestamp(credit.redeemedAt),
    raw_status: normalizeScalar(credit.status) || normalizeScalar(credit.state),
  }));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function normalizeScalar(value: unknown): string | undefined {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed || undefined;
  }
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return undefined;
}

function normalizeTimestamp(value: unknown): number | undefined {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value > 1_000_000_000_000 ? Math.floor(value / 1000) : Math.floor(value);
  }
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  if (!trimmed) return undefined;
  const numeric = Number(trimmed);
  if (Number.isFinite(numeric)) {
    return numeric > 1_000_000_000_000 ? Math.floor(numeric / 1000) : Math.floor(numeric);
  }
  const date = new Date(trimmed);
  return Number.isNaN(date.getTime()) ? undefined : Math.floor(date.getTime() / 1000);
}

function resetCreditStatusKey(credit: CodexResetCredit): "available" | "used" | "expired" | "unknown" {
  const status = (credit.status || credit.raw_status || "").trim().toLowerCase();
  if (["redeemed", "used", "consumed"].includes(status)) return "used";
  if (status === "expired") return "expired";
  if (status === "available" || !status) {
    const expiresAt = normalizeDate(credit.expires_at);
    return expiresAt && expiresAt.getTime() <= Date.now() ? "expired" : "available";
  }
  return "unknown";
}

function resetCreditStatusLabel(credit: CodexResetCredit): string {
  const key = resetCreditStatusKey(credit);
  if (key === "available") return "可用";
  if (key === "used") return "已使用";
  if (key === "expired") return "已过期";
  return credit.raw_status || credit.status || "未知";
}

function resetCreditDateLabel(value?: number): string {
  return formatDateTime(value) || "时间未知";
}

function resetCreditEndLabel(credit: CodexResetCredit): string {
  const usedAt = resetCreditDateLabel(credit.redeemed_at);
  if (resetCreditStatusKey(credit) === "used" && usedAt !== "时间未知") return `使用 ${usedAt}`;
  return `可用至 ${resetCreditDateLabel(credit.expires_at)}`;
}

function quotaWindowLabel(minutes?: number, fallback = "5h"): string {
  if (!minutes) return fallback;
  if (minutes % (60 * 24) === 0) return `${minutes / 60 / 24} 天窗口`;
  if (minutes % 60 === 0) return `${minutes / 60} 小时窗口`;
  return `${minutes} 分钟窗口`;
}

function quotaColor(value?: number): string {
  const percentage = value ?? 0;
  if (percentage >= 70) return "#22c55e";
  if (percentage >= 40) return "#f59e0b";
  return "#ef4444";
}

function quotaResetLeftLabel(value?: string | number): string {
  if (!value) return "--";
  const timestamp = Number(value);
  const date = Number.isFinite(timestamp)
    ? new Date(timestamp > 10_000_000_000 ? timestamp : timestamp * 1000)
    : new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  const diff = date.getTime() - Date.now();
  const abs = Math.max(0, diff);
  const day = Math.floor(abs / 86_400_000);
  const hour = Math.floor((abs % 86_400_000) / 3_600_000);
  const minute = Math.floor((abs % 3_600_000) / 60_000);
  return day > 0 ? `${day}d ${hour}h ${minute}m` : `${hour}h ${minute}m`;
}

function quotaResetDateLabel(value?: string | number): string {
  if (!value) return "等待刷新";
  const formatted = formatDateTime(value);
  return formatted ? `更新 ${formatted}` : String(value);
}

function formatRemainingTimeLabel(targetTime: number): string {
  const diff = targetTime - Date.now();
  if (diff <= 0) return "已过期";
  const totalMinutes = Math.floor(diff / 60_000);
  const days = Math.floor(totalMinutes / 1440);
  const hours = Math.floor((totalMinutes % 1440) / 60);
  const minutes = totalMinutes % 60;
  if (days > 0) return `${days}天${hours}小时`;
  return `${hours}小时${minutes}分钟`;
}

function expiryDaysLabel(value?: string): string {
  if (!value) return "";
  const date = normalizeDate(value);
  if (!date) return "";
  return formatRemainingTimeLabel(date.getTime());
}

function statusTitle(account: CodexAccount): string {
  return isApiKeyAccount(account) ? "密钥状态" : "订阅状态";
}

function quotaErrorMessage(account: CodexAccount): string {
  if (account.quota_error?.code === "token_expired") {
    return "Token 失效，请重新登录或更换绑定账号";
  }
  return account.quota_error?.message || "额度刷新失败";
}

function isTokenExpiredError(account: CodexAccount): boolean {
  return account.quota_error?.code === "token_expired";
}

function tokenExpiryStatus(value?: string): "valid" | "expired" {
  const date = normalizeDate(value);
  if (!date) return "valid";
  return date.getTime() <= Date.now() ? "expired" : "valid";
}

function normalizeDate(value?: string | number | null): Date | undefined {
  if (value === undefined || value === null || value === "") return undefined;
  const numeric = typeof value === "number" ? value : Number(value);
  const date = Number.isFinite(numeric)
    ? new Date(numeric > 10_000_000_000 ? numeric : numeric * 1000)
    : new Date(String(value));
  return Number.isNaN(date.getTime()) ? undefined : date;
}

function formatDateTime(value?: string | number | null): string {
  const date = normalizeDate(value);
  if (!date) return "";
  const pad = (input: number) => String(input).padStart(2, "0");
  return `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`;
}

function formatTime(value?: number | null): string {
  if (!value) return "从未使用";
  return formatDateTime(value);
}
</script>

<template>
  <a-spin class="accounts-spin" :loading="loading" dot>
    <section v-if="accounts.length" class="account-grid" :class="gridClass">
      <a-card
        v-for="account in accounts"
        :key="account.id"
        class="account-card"
        :class="{
          active: account.id === currentId,
          pinned: isPinned(account),
          draggable: settings.sortMode === 'custom',
          'drag-over': dragOverAccountId === account.id,
        }"
        :bordered="false"
        :draggable="settings.sortMode === 'custom'"
        @dragstart="handleDragStart($event, account)"
        @dragenter="handleDragOver($event, account)"
        @dragover="handleDragOver($event, account)"
        @dragleave="dragOverAccountId = ''"
        @dragend="handleDragEnd"
        @drop="handleDrop($event, account)"
      >
        <div class="account-head">
          <div class="account-title">
            <a-checkbox
              class="account-check"
              :model-value="selectedAccountIds.has(account.id)"
              @change="emit('toggle-account', account.id)"
            />
            <span
              class="account-name"
              :title="`${displayAccountName(account)}，双击复制邮箱`"
              @dblclick.stop="emit('copy-email', account)"
            >
              {{ displayAccountName(account) }}
            </span>
          </div>
          <div class="account-head-actions">
            <span v-if="account.id === currentId" class="current-account-pill">
              当前
            </span>
            <a-tooltip v-if="!isApiKeyAccount(account) && canUseResetCredit(account)" content="可用重置次数">
              <button
                class="reset-credit-pill"
                type="button"
                :disabled="quotaRefreshingId === account.id"
                @click.stop="emit('reset-credit', account)"
              >
                <icon-thunderbolt />
                {{ resetCreditCount(account) }}
              </button>
            </a-tooltip>
            <a-tooltip :content="isPinned(account) ? '取消置顶' : '置顶账号'">
              <button
                class="pin-button"
                :class="{ active: isPinned(account) }"
                type="button"
                @click.stop="emit('toggle-pin', account)"
              >
                <icon-pushpin />
              </button>
            </a-tooltip>
            <PlanBadge :label="planLabel(account)" :badge-class="planClass(account)" />
          </div>
        </div>

        <div v-if="isApiKeyAccount(account)" class="account-summary">
          <div class="chip-line">
            <a-button class="soft-chip" size="mini" @click="emit('open-binding', account)">
              <template #icon><icon-link /></template>
              {{ boundOAuthName(account) === "未绑定" ? "绑定 OAuth" : boundOAuthName(account) }}
            </a-button>
          </div>

          <div class="login-line">{{ accountLoginLine(account) }}</div>
          <div class="login-line full-url" :title="apiBaseUrl(account)">
            {{ apiBaseUrlLine(account) }}
          </div>
          <button
            v-if="apiOfficialUrl(account)"
            class="official-link"
            type="button"
            :title="apiOfficialUrl(account)"
            @click="emit('open-official-url', apiOfficialUrl(account))"
          >
            <icon-link />
            <span>
              <b>官网地址</b>
              <em>{{ apiOfficialUrl(account) }}</em>
            </span>
          </button>
        </div>

        <div class="account-health">
          <template v-if="shouldShowQuota(account) && account.quota">
            <div class="quota-panel">
              <div class="quota-panel-head">
                <span>额度概览</span>
                <small>自动同步</small>
              </div>
              <div class="quota-metrics" :class="{ single: isFreePlanAccount(account) }">
                <div v-if="account.quota.hourly_window_present !== false" class="quota-metric">
                  <div class="quota-metric-top">
                    <span>
                      <icon-calendar v-if="isFreePlanAccount(account)" />
                      <icon-clock-circle v-else />
                      {{ isFreePlanAccount(account) ? "长周期" : "短周期" }}
                    </span>
                    <strong :style="{ color: quotaColor(account.quota.hourly_percentage) }">
                      {{ account.quota.hourly_percentage }}%
                    </strong>
                  </div>
                  <div class="quota-bar">
                    <span
                      :style="{
                        width: `${Math.max(0, Math.min(100, account.quota.hourly_percentage))}%`,
                        background: quotaColor(account.quota.hourly_percentage),
                      }"
                    />
                  </div>
                  <div class="quota-meta">
                    <span>
                      <b>{{ quotaWindowLabel(account.quota.hourly_window_minutes, '5 小时窗口') }}</b>
                      <em>{{ quotaResetDateLabel(account.quota.hourly_reset_time) }}</em>
                      <small>{{ quotaResetLeftLabel(account.quota.hourly_reset_time) }}</small>
                    </span>
                  </div>
                </div>

                <div
                  v-if="!isFreePlanAccount(account) && account.quota.weekly_window_present !== false"
                  class="quota-metric"
                >
                  <div class="quota-metric-top">
                    <span>
                      <icon-calendar />
                      长周期
                    </span>
                    <strong :style="{ color: quotaColor(account.quota.weekly_percentage) }">
                      {{ account.quota.weekly_percentage }}%
                    </strong>
                  </div>
                  <div class="quota-bar">
                    <span
                      :style="{
                        width: `${Math.max(0, Math.min(100, account.quota.weekly_percentage))}%`,
                        background: quotaColor(account.quota.weekly_percentage),
                      }"
                    />
                  </div>
                  <div class="quota-meta">
                    <span>
                      <b>{{ quotaWindowLabel(account.quota.weekly_window_minutes, '7 天窗口') }}</b>
                      <em>{{ quotaResetDateLabel(account.quota.weekly_reset_time) }}</em>
                      <small>{{ quotaResetLeftLabel(account.quota.weekly_reset_time) }}</small>
                    </span>
                  </div>
                </div>

                <div
                  v-if="isFreePlanAccount(account) && account.quota.hourly_window_present !== false"
                  class="quota-metric quota-placeholder"
                  aria-hidden="true"
                />
              </div>
            </div>
          </template>
          <div
            v-else-if="shouldShowQuotaError(account) && account.quota_error"
            class="quota-error"
            :class="{ actionable: isTokenExpiredError(account) }"
          >
            <span>{{ quotaErrorMessage(account) }}</span>
            <a-button
              v-if="isTokenExpiredError(account)"
              size="mini"
              status="danger"
              @click="emit('reauthorize', account)"
            >
              重新授权
            </a-button>
          </div>

          <div
            v-if="!isBoundApiKeyAccount(account) && (accountSubscriptionUntil(account) || accountTokenExpiresAt(account))"
            class="status-grid"
            :class="{ single: !(accountSubscriptionUntil(account) && accountTokenExpiresAt(account)) }"
          >
            <div
              v-if="!accountSubscriptionUntil(account) && accountTokenExpiresAt(account)"
              class="status-card status-placeholder"
              aria-hidden="true"
            />

            <div v-if="accountSubscriptionUntil(account)" class="status-card status-valid">
              <span>
                <icon-calendar />
                {{ statusTitle(account) }}
                <b class="status-remaining-time">
                  {{ expiryDaysLabel(accountSubscriptionUntil(account)) || "已记录" }}
                </b>
              </span>
              <strong>{{ formatDateTime(accountSubscriptionUntil(account)) }}</strong>
            </div>

            <div v-if="accountTokenExpiresAt(account)" class="status-card status-token-expired">
              <span>
                <icon-clock-circle />
                Token {{ tokenExpiryStatus(accountTokenExpiresAt(account)) === "expired" ? "失效" : "可用" }}
              </span>
              <strong>{{ formatDateTime(accountTokenExpiresAt(account)) }}</strong>
            </div>
          </div>
        </div>

        <footer class="card-footer">
          <div class="footer-meta">
            <span>{{ formatTime(account.last_used) }}</span>
            <button
              v-if="!isApiKeyAccount(account) && account.bound_phone"
              class="footer-phone"
              type="button"
              @click="emit('open-phone', account)"
            >
              <icon-phone />
              {{ displayPhone(account.bound_phone) }}
            </button>
          </div>
          <div class="card-actions">
            <a-tooltip v-if="!isApiKeyAccount(account)" content="绑定手机">
              <a-button size="small" title="绑定手机" @click="emit('open-phone', account)">
                <template #icon><icon-phone /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip content="编辑">
              <a-button size="small" title="编辑" @click="emit('open-edit', account)">
                <template #icon><icon-edit /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip content="切换">
              <a-button
                size="small"
                title="切换"
                :loading="switchingId === account.id"
                @click="emit('switch-account', account)"
              >
                <template #icon><icon-play-arrow /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip v-if="!isApiKeyAccount(account)" content="刷新额度">
              <a-button
                size="small"
                title="刷新额度"
                :loading="quotaRefreshingId === account.id"
                @click="emit('refresh-quota', account)"
              >
                <template #icon><icon-refresh /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip v-if="canUseResetCredit(account)" content="重置额度">
              <a-button
                size="small"
                title="重置额度"
                :loading="quotaRefreshingId === account.id"
                @click="emit('reset-credit', account)"
              >
                <template #icon><icon-thunderbolt /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip content="导出">
              <a-button
                size="small"
                title="导出"
                :loading="exportingId === account.id"
                @click="emit('open-export', account)"
              >
                <template #icon><icon-download /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip content="删除">
              <a-button
                size="small"
                title="删除"
                :loading="deletingId === account.id"
                @click="emit('confirm-delete', account)"
              >
                <template #icon><icon-delete /></template>
              </a-button>
            </a-tooltip>
          </div>
        </footer>
      </a-card>
    </section>

    <section v-else class="empty-wrap">
      <div class="empty-panel">
        <div class="empty-copy">
          <span class="empty-kicker">本机还没有可切换账号</span>
          <h2>先放进一个 Codex 登录态</h2>
          <p>
            导入 OAuth Token / JSON，或添加 API Key。保存后这里会显示账号卡片，
            之后就可以一键切换并写回本机 Codex 配置。
          </p>
          <div class="empty-actions">
            <a-button type="primary" size="large" @click="emit('open-add', 'token')">
              <template #icon><icon-import /></template>
              导入 Token / JSON
            </a-button>
            <a-button size="large" @click="emit('open-add', 'apikey')">
              <template #icon><icon-plus /></template>
              添加 API Key
            </a-button>
          </div>
          <div class="empty-steps" aria-label="账号添加流程">
            <span>粘贴凭据</span>
            <span>保存账号</span>
            <span>切换 Codex</span>
          </div>
        </div>

        <div class="empty-preview" aria-hidden="true">
          <div class="preview-card preview-card-main">
            <div class="preview-head">
              <span class="preview-dot" />
              <span class="preview-name">account@example.com</span>
              <span class="preview-badge">OAUTH</span>
            </div>
            <div class="preview-line wide" />
            <div class="preview-line" />
            <div class="preview-footer">
              <span />
              <span />
              <span />
            </div>
          </div>
          <div class="preview-card preview-card-back">
            <div class="preview-head">
              <span class="preview-dot muted" />
              <span class="preview-name">api****610</span>
              <span class="preview-badge api">API_KEY</span>
            </div>
            <div class="preview-line wide" />
            <div class="preview-line short" />
          </div>
        </div>
      </div>
    </section>
  </a-spin>
</template>
