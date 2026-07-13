<script setup lang="ts">
import { Message } from "@arco-design/web-vue";
import { computed, ref } from "vue";
import type { CodexSwitcherSettings } from "../services/codex";
import type { CodexAccount, CodexResetCredit } from "../types/codex";
import { formatLocalizedDuration, t } from "../i18n";
import { hasAnyQuotaWindow, hasQuotaWindow } from "../quota";
import PlanBadge from "./PlanBadge.vue";

const props = defineProps<{
  accounts: CodexAccount[];
  hasAnyAccount: boolean;
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
  (event: "open-models", account: CodexAccount): void;
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

const accountViewMode = computed(() => {
  const mode = props.settings.accountViewMode;
  return mode === "compact" || mode === "table" ? mode : "card";
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

function handleCardClick(event: MouseEvent, account: CodexAccount): void {
  if (isInteractiveClick(event)) return;
  emit("toggle-account", account.id);
}

function isInteractiveClick(event: MouseEvent): boolean {
  const target = event.target;
  if (!(target instanceof HTMLElement)) return false;
  return Boolean(
    target.closest(
      [
        "button",
        "a",
        "input",
        "textarea",
        "select",
        "label",
        "[role='button']",
        "[contenteditable='true']",
        ".arco-checkbox",
        ".arco-btn",
        ".arco-trigger",
        ".card-actions",
      ].join(","),
    ),
  );
}

function isApiKeyAccount(account: CodexAccount): boolean {
  return account.auth_mode === "apikey" || Boolean(account.openai_api_key || account.openaiApiKey);
}

function displayName(account: CodexAccount): string {
  return account.account_name || account.email || account.id || t("未命名账号");
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

function displayAccountEmail(account: CodexAccount): string {
  return displayIdentity(account.email || displayName(account));
}

function boundOAuthAccount(account: CodexAccount): CodexAccount | undefined {
  if (!account.bound_oauth_account_id) return undefined;
  return props.accounts.find((item) => item.id === account.bound_oauth_account_id);
}

function boundOAuthName(account: CodexAccount): string {
  const bound = boundOAuthAccount(account);
  return bound ? displayAccountName(bound) : t("未绑定");
}

function isBoundApiKeyAccount(account: CodexAccount): boolean {
  return isApiKeyAccount(account) && Boolean(account.bound_oauth_account_id);
}

function canShowQuota(account: CodexAccount): boolean {
  if (!props.settings.monitorQuota) return false;
  return !isApiKeyAccount(account);
}

function shouldShowQuota(account: CodexAccount): boolean {
  return canShowQuota(account) && hasAnyQuotaWindow(account.quota);
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

function accountAuthLine(account: CodexAccount): string {
  if (isApiKeyAccount(account)) return accountLoginLine(account);
  return account.tokens.refresh_token ? t("OAuth 登录") : t("Token 登录");
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
  return value ? `Base URL: ${value}` : `Base URL: ${t("未设置")}`;
}

function apiOfficialUrl(account: CodexAccount): string {
  return (account.api_official_url || account.apiOfficialUrl)?.trim() ?? "";
}

function apiOfficialHost(account: CodexAccount): string {
  const value = apiOfficialUrl(account);
  if (!value) return "";
  try {
    const url = new URL(value);
    return `${url.protocol}//${url.host}`;
  } catch {
    return value.replace(/\/+$/, "");
  }
}

async function copyText(value: string, successLabel: string): Promise<void> {
  const text = value.trim();
  if (!text) {
    Message.warning(t("没有可复制的内容"));
    return;
  }
  try {
    await navigator.clipboard.writeText(text);
    Message.success(t(successLabel));
  } catch {
    Message.error(t("复制失败，请手动选择内容复制"));
  }
}

function maskSecret(value?: string): string {
  const trimmed = value?.trim() ?? "";
  if (!trimmed) return t("未保存");
  if (trimmed.length <= 10) return `${trimmed.slice(0, 3)}****`;
  return `${trimmed.slice(0, 6)}****${trimmed.slice(-4)}`;
}

function canUseResetCredit(account: CodexAccount): boolean {
  return canShowQuota(account) && Boolean(account.quota) && resetCreditCount(account) > 0;
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
  if (key === "available") return t("可用");
  if (key === "used") return t("已使用");
  if (key === "expired") return t("已过期");
  return credit.raw_status || credit.status || t("未知");
}

function resetCreditDateLabel(value?: number): string {
  return formatDateTime(value) || t("时间未知");
}

function resetCreditEndLabel(credit: CodexResetCredit): string {
  const usedAt = resetCreditDateLabel(credit.redeemed_at);
  if (resetCreditStatusKey(credit) === "used" && usedAt !== t("时间未知")) return `${t("使用")} ${usedAt}`;
  return `${t("可用至")} ${resetCreditDateLabel(credit.expires_at)}`;
}

function quotaWindowShortLabel(minutes?: number, fallbackMinutes = 300): string {
  const value = minutes || fallbackMinutes;
  if (value % (60 * 24) === 0) return `${value / 60 / 24} ${t("天")}`;
  if (value % 60 === 0) return `${value / 60} ${t("小时")}`;
  return `${value} ${t("分钟")}`;
}

function quotaColor(value?: number): string {
  const percentage = value ?? 0;
  if (percentage >= 70) return "#22c55e";
  if (percentage >= 40) return "#f59e0b";
  return "#ef4444";
}

function quotaPercentLabel(value?: number): string {
  return Number.isFinite(value) ? `${value}%` : "--";
}

function quotaDotStyle(value?: number): Record<string, string> {
  return { background: quotaColor(value) };
}

function quotaProgressStyle(value?: number): Record<string, string> {
  const width = Number.isFinite(value) ? Math.max(0, Math.min(100, Number(value))) : 0;
  return {
    width: `${width}%`,
    background: quotaColor(value),
  };
}

function accountSubscriptionClass(account: CodexAccount): string {
  const value = accountSubscriptionUntil(account);
  if (!value) return "unknown";
  const date = normalizeDate(value);
  if (date && date.getTime() <= Date.now()) return "expired";
  return "active";
}

function accountSubscriptionBadge(account: CodexAccount): string {
  const value = accountSubscriptionUntil(account);
  if (!value) return t("未获得订阅信息");
  const date = normalizeDate(value);
  if (date && date.getTime() <= Date.now()) return t("已过期");
  return expiryDaysLabel(value) || t("已记录");
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
  if (!value) return t("等待刷新");
  const formatted = formatDateTime(value);
  return formatted ? t(`更新 ${formatted}`) : String(value);
}

function formatRemainingTimeLabel(targetTime: number): string {
  const diff = targetTime - Date.now();
  if (diff <= 0) return t("已过期");
  const totalMinutes = Math.floor(diff / 60_000);
  const days = Math.floor(totalMinutes / 1440);
  const hours = Math.floor((totalMinutes % 1440) / 60);
  const minutes = totalMinutes % 60;
  return formatLocalizedDuration(days, hours, minutes);
}

function expiryDaysLabel(value?: string): string {
  if (!value) return "";
  const date = normalizeDate(value);
  if (!date) return "";
  return formatRemainingTimeLabel(date.getTime());
}

function statusTitle(account: CodexAccount): string {
  return isApiKeyAccount(account) ? t("密钥状态") : t("订阅状态");
}

function quotaErrorMessage(account: CodexAccount): string {
  if (account.quota_error?.code === "token_expired") {
    return t("Token 失效，请重新登录或更换绑定账号");
  }
  return account.quota_error?.message ? t(account.quota_error.message) : t("额度刷新失败");
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
  if (!value) return t("从未使用");
  return formatDateTime(value);
}
</script>

<template>
  <a-spin class="accounts-spin" :loading="loading" dot>
    <section v-if="accounts.length && accountViewMode === 'card'" class="account-grid" :class="gridClass">
      <a-card
        v-for="account in accounts"
        :key="account.id"
        class="account-card"
        :class="{
          active: account.id === currentId,
          pinned: isPinned(account),
          'api-key-account': isApiKeyAccount(account),
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
        @click="handleCardClick($event, account)"
      >
        <div class="account-head">
          <div class="account-identity-card">
            <div class="account-check-zone" @click.stop>
              <a-checkbox
                class="account-check"
                :model-value="selectedAccountIds.has(account.id)"
                @change="emit('toggle-account', account.id)"
              />
            </div>
            <div class="account-title">
              <div class="account-title-main">
                <span
                  class="account-name"
                  :title="`${displayAccountEmail(account)}, ${t('双击复制邮箱')}`"
                  @dblclick.stop="emit('copy-email', account)"
                >
                  {{ displayAccountEmail(account) }}
                </span>
              </div>
              <div class="account-action-meta">
                <small>{{ t("账号 ID") }}: {{ shortAccountId(account) }}</small>
                <span v-if="account.id === currentId" class="current-account-pill identity-current-pill">
                  {{ t("当前") }}
                </span>
                <span v-else class="identity-current-placeholder" aria-hidden="true" />
                <PlanBadge :label="planLabel(account)" :badge-class="planClass(account)" />
              </div>
            </div>
          </div>
        </div>

        <div v-if="isApiKeyAccount(account)" class="account-summary api-key-summary">
          <div class="chip-line api-bind-line">
            <a-button class="soft-chip api-bind-chip" size="mini" @click.stop="emit('open-binding', account)">
              <template #icon><icon-link /></template>
              {{ boundOAuthName(account) === t("未绑定") ? t("绑定 OAuth") : boundOAuthName(account) }}
            </a-button>
          </div>

          <div class="api-key-info-card">
            <div class="api-key-info-row">
              <span class="api-key-info-icon api-key-info-icon-key"><icon-link /></span>
              <span class="api-key-info-text">
                <b>API Key</b>
                <em>{{ maskSecret(account.openai_api_key || account.openaiApiKey) }}</em>
              </span>
              <a-tooltip :content="t('复制 API Key')">
                <a-button
                  class="api-key-copy-btn"
                  size="small"
                  :title="t('复制 API Key')"
                  @click.stop="copyText(account.openai_api_key || account.openaiApiKey || '', '已复制 API Key')"
                >
                  <template #icon><icon-copy /></template>
                </a-button>
              </a-tooltip>
            </div>
            <div class="api-key-info-row">
              <span class="api-key-info-icon api-key-info-icon-url"><icon-link /></span>
              <span class="api-key-info-text">
                <b>Base URL</b>
                <em :title="apiBaseUrl(account)">{{ apiBaseUrl(account) || t("未设置") }}</em>
              </span>
              <a-tooltip :content="t('复制 Base URL')">
                <a-button
                  class="api-key-copy-btn"
                  size="small"
                  :title="t('复制 Base URL')"
                  @click.stop="copyText(apiBaseUrl(account), '已复制 Base URL')"
                >
                  <template #icon><icon-copy /></template>
                </a-button>
              </a-tooltip>
            </div>
          </div>
          <button
            v-if="apiOfficialUrl(account)"
            class="official-link"
            type="button"
            :title="apiOfficialUrl(account)"
            @click.stop="emit('open-official-url', apiOfficialUrl(account))"
          >
            <icon-link />
            <span>
              <b>{{ t("官网地址") }}</b>
              <em>{{ apiOfficialHost(account) }}</em>
            </span>
            <icon-link />
          </button>
        </div>

        <div v-if="!isApiKeyAccount(account)" class="account-health">
          <template v-if="shouldShowQuota(account) && account.quota">
            <div class="quota-panel">
              <div class="quota-panel-head">
                <span>{{ t("额度概览") }}</span>
                <small>{{ t("自动同步") }}</small>
              </div>
              <div class="quota-metrics" :class="{ single: isFreePlanAccount(account) }">
                <div v-if="hasQuotaWindow(account.quota, 'hourly')" class="quota-metric">
                  <div class="quota-metric-main">
                    <span class="quota-window-label">
                      <icon-calendar v-if="isFreePlanAccount(account)" />
                      <icon-clock-circle v-else />
                      {{ quotaWindowShortLabel(account.quota.hourly_window_minutes, isFreePlanAccount(account) ? 43200 : 300) }}
                    </span>
                    <em>{{ quotaResetDateLabel(account.quota.hourly_reset_time) }}</em>
                    <small>{{ quotaResetLeftLabel(account.quota.hourly_reset_time) }}</small>
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
                </div>

                <div
                  v-if="!isFreePlanAccount(account) && hasQuotaWindow(account.quota, 'weekly')"
                  class="quota-metric"
                >
                  <div class="quota-metric-main">
                    <span class="quota-window-label">
                      <icon-calendar />
                      {{ quotaWindowShortLabel(account.quota.weekly_window_minutes, 10080) }}
                    </span>
                    <em>{{ quotaResetDateLabel(account.quota.weekly_reset_time) }}</em>
                    <small>{{ quotaResetLeftLabel(account.quota.weekly_reset_time) }}</small>
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
                </div>

                <div
                  v-if="isFreePlanAccount(account) && hasQuotaWindow(account.quota, 'hourly')"
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
              {{ t("重新授权") }}
            </a-button>
          </div>

          <div v-if="!isBoundApiKeyAccount(account)" class="status-grid">
            <div
              class="status-card"
              :class="accountSubscriptionUntil(account) ? 'status-valid' : 'status-placeholder'"
              :aria-hidden="accountSubscriptionUntil(account) ? undefined : 'true'"
            >
              <template v-if="accountSubscriptionUntil(account)">
                <span>
                  <icon-calendar />
                  {{ statusTitle(account) }}
                  <b class="status-remaining-time">
                    {{ expiryDaysLabel(accountSubscriptionUntil(account)) || t("已记录") }}
                  </b>
                </span>
                <strong>{{ formatDateTime(accountSubscriptionUntil(account)) }}</strong>
              </template>
            </div>

            <div
              class="status-card"
              :class="accountTokenExpiresAt(account) ? 'status-token-expired' : 'status-placeholder'"
              :aria-hidden="accountTokenExpiresAt(account) ? undefined : 'true'"
            >
              <template v-if="accountTokenExpiresAt(account)">
                <span>
                  <icon-clock-circle />
                  {{ tokenExpiryStatus(accountTokenExpiresAt(account)) === "expired" ? t("Token 失效") : t("Token 可用") }}
                </span>
                <strong>{{ formatDateTime(accountTokenExpiresAt(account)) }}</strong>
              </template>
            </div>
          </div>
        </div>

        <footer class="card-footer">
          <div class="footer-meta">
            <small class="account-last-used">{{ formatTime(account.last_used) }}</small>
            <button
              v-if="!isApiKeyAccount(account) && account.bound_phone"
              class="footer-phone"
              type="button"
              @click="emit('open-phone', account)"
            >
              <icon-phone />
              <span>{{ displayPhone(account.bound_phone) }}</span>
            </button>
          </div>
          <div class="card-actions" @click.stop>
            <a-tooltip :content="isPinned(account) ? t('取消置顶') : t('置顶账号')">
              <a-button
                size="small"
                :title="isPinned(account) ? t('取消置顶') : t('置顶账号')"
                :class="{ 'action-active': isPinned(account) }"
                @click="emit('toggle-pin', account)"
              >
                <template #icon><icon-pushpin /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip v-if="canUseResetCredit(account)" :content="t('重置额度')">
              <a-button
                size="small"
                :title="`${t('重置额度')} ${resetCreditCount(account)}`"
                :loading="quotaRefreshingId === account.id"
                @click="emit('reset-credit', account)"
              >
                <template #icon><icon-thunderbolt /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip v-if="!isApiKeyAccount(account)" :content="t('绑定手机')">
              <a-button size="small" :title="t('绑定手机')" @click="emit('open-phone', account)">
                <template #icon><icon-phone /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip :content="t('编辑')">
              <a-button size="small" :title="t('编辑')" @click="emit('open-edit', account)">
                <template #icon><icon-edit /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip v-if="isApiKeyAccount(account)" :content="t('获取模型列表')">
              <a-button size="small" :title="t('获取模型列表')" @click="emit('open-models', account)">
                <template #icon><icon-list /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip :content="t('切换')">
              <a-button
                size="small"
                :title="t('切换')"
                :loading="switchingId === account.id"
                @click="emit('switch-account', account)"
              >
                <template #icon><icon-play-arrow /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip v-if="!isApiKeyAccount(account)" :content="t('刷新额度')">
              <a-button
                size="small"
                :title="t('刷新额度')"
                :loading="quotaRefreshingId === account.id"
                @click="emit('refresh-quota', account)"
              >
                <template #icon><icon-refresh /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip :content="t('导出')">
              <a-button
                size="small"
                :title="t('导出')"
                :loading="exportingId === account.id"
                @click="emit('open-export', account)"
              >
                <template #icon><icon-download /></template>
              </a-button>
            </a-tooltip>
            <a-tooltip :content="t('删除')">
              <a-button
                size="small"
                :title="t('删除')"
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

    <section v-else-if="accounts.length && accountViewMode === 'compact'" class="account-compact-grid">
      <div
        v-for="account in accounts"
        :key="account.id"
        class="account-compact-row"
        :class="{
          active: account.id === currentId,
          pinned: isPinned(account),
          draggable: settings.sortMode === 'custom',
          'drag-over': dragOverAccountId === account.id,
        }"
        :draggable="settings.sortMode === 'custom'"
        @dragstart="handleDragStart($event, account)"
        @dragenter="handleDragOver($event, account)"
        @dragover="handleDragOver($event, account)"
        @dragleave="dragOverAccountId = ''"
        @dragend="handleDragEnd"
        @drop="handleDrop($event, account)"
      >
        <a-checkbox
          class="account-check"
          :model-value="selectedAccountIds.has(account.id)"
          @change="emit('toggle-account', account.id)"
        />
        <button
          class="compact-account-name"
          type="button"
          :title="`${displayAccountName(account)}, ${t('双击复制邮箱')}`"
          @dblclick.stop="emit('copy-email', account)"
        >
          {{ displayAccountName(account) }}
        </button>
        <span v-if="account.id === currentId" class="current-account-pill compact-current">{{ t("当前") }}</span>
        <span v-if="shouldShowQuota(account) && account.quota" class="compact-quota-pair">
          <span v-if="hasQuotaWindow(account.quota, 'hourly')">
            <i :style="quotaDotStyle(account.quota.hourly_percentage)" />
            {{ quotaPercentLabel(account.quota.hourly_percentage) }}
          </span>
          <span v-if="!isFreePlanAccount(account) && hasQuotaWindow(account.quota, 'weekly')">
            <i :style="quotaDotStyle(account.quota.weekly_percentage)" />
            {{ quotaPercentLabel(account.quota.weekly_percentage) }}
          </span>
        </span>
        <span v-else-if="shouldShowQuotaError(account)" class="compact-quota-error">
          {{ t("异常") }}
        </span>
        <span v-else class="compact-quota-pair muted">
          <span><i />--</span>
          <span><i />--</span>
        </span>
        <span
          v-if="accountSubscriptionUntil(account)"
          class="compact-subscription"
          :class="accountSubscriptionClass(account)"
        >
          {{ accountSubscriptionBadge(account) }}
        </span>
        <a-tooltip v-if="!isApiKeyAccount(account)" :content="t('刷新额度')">
          <button
            class="compact-text-action"
            type="button"
            :disabled="quotaRefreshingId === account.id"
            @click="emit('refresh-quota', account)"
          >
            {{ t("刷新") }}
          </button>
        </a-tooltip>
        <PlanBadge :label="planLabel(account)" :badge-class="planClass(account)" />
        <a-tooltip :content="isPinned(account) ? t('取消置顶') : t('置顶账号')">
          <button
            class="compact-icon-action"
            :class="{ active: isPinned(account) }"
            type="button"
            @click.stop="emit('toggle-pin', account)"
          >
            <icon-pushpin />
          </button>
        </a-tooltip>
        <a-tooltip :content="t('编辑')">
          <button class="compact-icon-action" type="button" @click="emit('open-edit', account)">
            <icon-file />
          </button>
        </a-tooltip>
        <a-tooltip v-if="isApiKeyAccount(account)" :content="t('获取模型列表')">
          <button class="compact-icon-action" type="button" @click="emit('open-models', account)">
            <icon-list />
          </button>
        </a-tooltip>
        <a-tooltip :content="t('切换')">
          <button
            class="compact-icon-action primary"
            type="button"
            :disabled="switchingId === account.id"
            @click="emit('switch-account', account)"
          >
            <icon-play-arrow />
          </button>
        </a-tooltip>
      </div>
    </section>

    <section v-else-if="accounts.length && accountViewMode === 'table'" class="account-table-wrap">
      <table class="account-table">
        <thead>
          <tr>
            <th class="account-table-check"></th>
            <th>{{ t("邮箱") }}</th>
            <th>{{ t("订阅") }}</th>
            <th>{{ t("订阅信息") }}</th>
            <th>{{ t("配额状态") }}</th>
            <th>{{ t("操作") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="account in accounts"
            :key="account.id"
            :class="{
              active: account.id === currentId,
              draggable: settings.sortMode === 'custom',
              'drag-over': dragOverAccountId === account.id,
            }"
            :draggable="settings.sortMode === 'custom'"
            @dragstart="handleDragStart($event, account)"
            @dragenter="handleDragOver($event, account)"
            @dragover="handleDragOver($event, account)"
            @dragleave="dragOverAccountId = ''"
            @dragend="handleDragEnd"
            @drop="handleDrop($event, account)"
          >
            <td class="account-table-check">
              <a-checkbox
                :model-value="selectedAccountIds.has(account.id)"
                @change="emit('toggle-account', account.id)"
              />
            </td>
            <td>
              <div class="table-account-main">
                <button
                  type="button"
                  :title="`${displayAccountName(account)}, ${t('双击复制邮箱')}`"
                  @dblclick.stop="emit('copy-email', account)"
                >
                  {{ displayAccountName(account) }}
                </button>
                <span>
                  {{ accountAuthLine(account) }}
                  <template v-if="account.id === currentId"> · {{ t("当前") }}</template>
                </span>
                <small>{{ t("用户 ID") }}: {{ shortAccountId(account) }}</small>
              </div>
            </td>
            <td>
              <PlanBadge :label="planLabel(account)" :badge-class="planClass(account)" />
            </td>
            <td>
              <div class="table-subscription">
                <span :class="accountSubscriptionClass(account)">
                  {{ accountSubscriptionBadge(account) }}
                </span>
                <small v-if="accountSubscriptionUntil(account)">
                  {{ formatDateTime(accountSubscriptionUntil(account)) }}
                </small>
              </div>
            </td>
            <td>
              <div v-if="shouldShowQuota(account) && account.quota" class="table-quota-stack">
                <div v-if="hasQuotaWindow(account.quota, 'hourly')" class="table-quota-line">
                  <span>
                    {{ quotaWindowShortLabel(account.quota.hourly_window_minutes, isFreePlanAccount(account) ? 43200 : 300) }}
                  </span>
                  <div><i :style="quotaProgressStyle(account.quota.hourly_percentage)" /></div>
                  <strong :style="{ color: quotaColor(account.quota.hourly_percentage) }">
                    {{ quotaPercentLabel(account.quota.hourly_percentage) }}
                  </strong>
                  <small>{{ quotaResetLeftLabel(account.quota.hourly_reset_time) }}</small>
                </div>
                <div
                  v-if="!isFreePlanAccount(account) && hasQuotaWindow(account.quota, 'weekly')"
                  class="table-quota-line"
                >
                  <span>{{ t("周配额") }}</span>
                  <div><i :style="quotaProgressStyle(account.quota.weekly_percentage)" /></div>
                  <strong :style="{ color: quotaColor(account.quota.weekly_percentage) }">
                    {{ quotaPercentLabel(account.quota.weekly_percentage) }}
                  </strong>
                  <small>{{ quotaResetLeftLabel(account.quota.weekly_reset_time) }}</small>
                </div>
              </div>
              <div
                v-else-if="shouldShowQuotaError(account) && account.quota_error"
                class="table-quota-error"
              >
                <span>{{ quotaErrorMessage(account) }}</span>
                <a-button
                  v-if="isTokenExpiredError(account)"
                  size="mini"
                  status="danger"
                  @click="emit('reauthorize', account)"
                >
                  {{ t("重新授权") }}
                </a-button>
              </div>
              <span v-else class="table-muted">{{ t("未获得订阅信息") }}</span>
            </td>
            <td>
              <div class="table-actions">
                <a-tooltip v-if="!isApiKeyAccount(account)" :content="t('绑定手机')">
                  <a-button size="mini" @click="emit('open-phone', account)">
                    <template #icon><icon-phone /></template>
                  </a-button>
                </a-tooltip>
                <a-tooltip :content="isPinned(account) ? t('取消置顶') : t('置顶账号')">
                  <a-button size="mini" @click.stop="emit('toggle-pin', account)">
                    <template #icon><icon-pushpin /></template>
                  </a-button>
                </a-tooltip>
                <a-tooltip :content="t('编辑')">
                  <a-button size="mini" @click="emit('open-edit', account)">
                    <template #icon><icon-file /></template>
                  </a-button>
                </a-tooltip>
                <a-tooltip v-if="isApiKeyAccount(account)" :content="t('获取模型列表')">
                  <a-button size="mini" @click="emit('open-models', account)">
                    <template #icon><icon-list /></template>
                  </a-button>
                </a-tooltip>
                <a-tooltip :content="t('切换')">
                  <a-button
                    size="mini"
                    :loading="switchingId === account.id"
                    @click="emit('switch-account', account)"
                  >
                    <template #icon><icon-play-arrow /></template>
                  </a-button>
                </a-tooltip>
                <a-tooltip v-if="!isApiKeyAccount(account)" :content="t('刷新额度')">
                  <a-button
                    size="mini"
                    :loading="quotaRefreshingId === account.id"
                    @click="emit('refresh-quota', account)"
                  >
                    <template #icon><icon-refresh /></template>
                  </a-button>
                </a-tooltip>
                <a-tooltip v-if="canUseResetCredit(account)" :content="t('重置额度')">
                  <a-button
                    size="mini"
                    :loading="quotaRefreshingId === account.id"
                    @click="emit('reset-credit', account)"
                  >
                    <template #icon><icon-thunderbolt /></template>
                  </a-button>
                </a-tooltip>
                <a-tooltip :content="t('导出')">
                  <a-button
                    size="mini"
                    :loading="exportingId === account.id"
                    @click="emit('open-export', account)"
                  >
                    <template #icon><icon-download /></template>
                  </a-button>
                </a-tooltip>
                <a-tooltip :content="t('删除')">
                  <a-button
                    size="mini"
                    :loading="deletingId === account.id"
                    @click="emit('confirm-delete', account)"
                  >
                    <template #icon><icon-delete /></template>
                  </a-button>
                </a-tooltip>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </section>

    <section v-else-if="hasAnyAccount" class="account-filter-empty">
      <a-empty :description="t('无数据')" />
    </section>

    <section v-else class="empty-wrap">
      <div class="empty-panel">
        <div class="empty-copy">
          <span class="empty-kicker">{{ t("本机还没有可切换账号") }}</span>
          <h2>{{ t("先放进一个 Codex 登录态") }}</h2>
          <p>
            {{ t("导入 OAuth Token / JSON，或添加 API Key。保存后这里会显示账号卡片，之后就可以一键切换并写回本机 Codex 配置。") }}
          </p>
          <div class="empty-actions">
            <a-button type="primary" size="large" @click="emit('open-add', 'token')">
              <template #icon><icon-import /></template>
              {{ t("导入 Token / JSON") }}
            </a-button>
            <a-button size="large" @click="emit('open-add', 'apikey')">
              <template #icon><icon-plus /></template>
              {{ t("添加 API Key") }}
            </a-button>
          </div>
          <div class="empty-steps" :aria-label="t('账号添加流程')">
            <span>{{ t("粘贴凭据") }}</span>
            <span>{{ t("保存账号") }}</span>
            <span>{{ t("切换 Codex") }}</span>
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
