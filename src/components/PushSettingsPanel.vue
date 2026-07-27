<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { Message } from "@arco-design/web-vue";
import { t } from "../i18n";
import { hasAnyQuotaWindow, hasQuotaWindow } from "../quota";
import {
  clearPushLogs,
  countSuccessfulPushLogsSince,
  createPushChannel,
  createPushRule,
  defaultPushSettings,
  getPushSettings,
  listPushLogs,
  runPushNow,
  runPushRuleNow,
  testPushChannel,
  updatePushSettings,
  type PushChannelConfig,
  type PushChannelType,
  type PushLogEntry,
  type PushRule,
  type PushRunSummary,
  type PushSettings,
} from "../services/push";
import type { CodexAccount } from "../types/codex";
import PlanBadge from "./PlanBadge.vue";

const props = defineProps<{
  accounts: CodexAccount[];
  displayName: (account: CodexAccount) => string;
  planLabel: (account: CodexAccount) => string;
  planClass: (account: CodexAccount) => string;
  privacyMasked: boolean;
}>();

const emit = defineEmits<{
  (event: "back"): void;
  (event: "accounts-refreshed"): void;
}>();

const settings = reactive<PushSettings>(defaultPushSettings());
const loading = ref(false);
const settingsLoaded = ref(false);
const saving = ref(false);
const runningRuleId = ref("");
const testingChannelId = ref("");
const logsLoading = ref(false);
const logs = ref<PushLogEntry[]>([]);
const todayDeliveryCount = ref(0);
const activeTab = ref("rules");
const editingRuleId = ref("");
const selectedChannelId = ref("");
const channelSearch = ref("");
const savedChannelSnapshots = ref<Record<string, PushChannelConfig>>({});
const savedSettingsSnapshot = ref<PushSettings>(defaultPushSettings());
const accountPickerVisible = ref(false);
const accountPickerRuleId = ref("");
const accountPickerSearch = ref("");
const accountPickerSelection = ref<Set<string>>(new Set());

const channelOptions: Array<{ value: PushChannelType; label: string }> = [
  { value: "serverChan", label: "Server酱" },
  { value: "pushPlus", label: "PushPlus" },
  { value: "enterpriseWechat", label: "企业微信" },
  { value: "wxPusher", label: "WxPusher" },
  { value: "bark", label: "Bark" },
  { value: "chanify", label: "Chanify" },
  { value: "pushDeer", label: "PushDeer" },
  { value: "dingTalk", label: "钉钉" },
];

const enabledChannels = computed(() => settings.channels.filter((channel) => channel.enabled));
const enabledRules = computed(() => settings.rules.filter((rule) => rule.enabled));
const editingRule = computed(() => settings.rules.find((rule) => rule.id === editingRuleId.value));
const selectedChannel = computed(() => settings.channels.find((channel) => channel.id === selectedChannelId.value));
const filteredChannels = computed(() => {
  const keyword = channelSearch.value.trim().toLocaleLowerCase();
  if (!keyword) return settings.channels;
  return settings.channels.filter((channel) =>
    [channelTypeLabel(channel), channel.nickname]
      .some((value) => value.toLocaleLowerCase().includes(keyword)),
  );
});
const accountPickerRule = computed(() => settings.rules.find((rule) => rule.id === accountPickerRuleId.value));
const accountOptions = computed(() => props.accounts.filter(canPushAccount));
const accountOptionIds = computed(() => new Set(accountOptions.value.map((account) => account.id)));
const filteredAccountPickerOptions = computed(() => {
  const keyword = accountPickerSearch.value.trim().toLocaleLowerCase();
  if (!keyword) return accountOptions.value;
  return accountOptions.value.filter((account) =>
    [props.displayName(account), account.account_name, account.email, account.api_provider_name, account.apiProviderName]
      .some((value) => (value || "").toLocaleLowerCase().includes(keyword)),
  );
});
const selectedAccountPickerCount = computed(() =>
  accountOptions.value.filter((account) => accountPickerSelection.value.has(account.id)).length,
);
const selectedVisibleAccountPickerCount = computed(() =>
  filteredAccountPickerOptions.value.filter((account) => accountPickerSelection.value.has(account.id)).length,
);
const allVisiblePickerAccountsSelected = computed(() =>
  filteredAccountPickerOptions.value.length > 0
    && selectedVisibleAccountPickerCount.value === filteredAccountPickerOptions.value.length,
);
const pickerSelectionIndeterminate = computed(() =>
  selectedVisibleAccountPickerCount.value > 0
    && selectedVisibleAccountPickerCount.value < filteredAccountPickerOptions.value.length,
);

onMounted(() => void load());

function errorText(error: unknown): string {
  return String(error instanceof Error ? error.message : error).replace(/^Error:\s*/, "");
}

function isApiKeyAccount(account: CodexAccount): boolean {
  return account.auth_mode === "apikey" || Boolean(account.openai_api_key || account.openaiApiKey);
}

function canPushAccount(account: CodexAccount): boolean {
  return !isApiKeyAccount(account) || Boolean(account.bound_oauth_account_id);
}

function accountLabel(account: CodexAccount): string {
  const label = props.displayName(account);
  if (!props.privacyMasked) return label;
  const [name = "", domain = ""] = account.email.split("@");
  return domain ? `${name.slice(0, 2)}***@${domain}` : `${label.slice(0, 3)}****`;
}

function privacyLogAccountLabel(value: string): string {
  return props.privacyMasked ? t("已隐藏账号") : value;
}

function privacyLogContent(value: string): string {
  return props.privacyMasked ? t("隐私模式下已隐藏推送内容") : value;
}

function accountStatusSource(account: CodexAccount): CodexAccount {
  if (!isApiKeyAccount(account) || !account.bound_oauth_account_id) return account;
  return props.accounts.find((candidate) => candidate.id === account.bound_oauth_account_id) || account;
}

function accountSecondaryLabel(account: CodexAccount): string {
  if (isApiKeyAccount(account)) {
    const provider = account.api_provider_name || account.apiProviderName || t("自定义服务");
    const baseUrl = account.api_base_url || account.apiBaseUrl || "https://api.openai.com/v1";
    return `API Key · ${provider} · ${baseUrl}`;
  }
  return `OAuth · ${props.privacyMasked ? accountLabel(account) : account.email || account.id}`;
}

function isFreePlanAccount(account: CodexAccount): boolean {
  return !isApiKeyAccount(account) && (account.plan_type || "").trim().toLowerCase().includes("free");
}

function quotaWindowLabel(minutes?: number, fallbackMinutes = 300): string {
  const safeMinutes = minutes && Number.isFinite(minutes) ? minutes : fallbackMinutes;
  if (safeMinutes % (60 * 24) === 0) return `${safeMinutes / 60 / 24} ${t("天窗口")}`;
  if (safeMinutes % 60 === 0) return `${safeMinutes / 60} ${t("小时窗口")}`;
  return `${safeMinutes} ${t("分钟窗口")}`;
}

function quotaColor(value?: number): string {
  const percentage = value ?? 0;
  if (percentage >= 70) return "#22c55e";
  if (percentage >= 40) return "#f59e0b";
  return "#ef4444";
}

function quotaResetLeftLabel(value?: number): string {
  if (!value || !Number.isFinite(value)) return "--";
  const timestamp = value > 10_000_000_000 ? value : value * 1000;
  const remaining = Math.max(0, timestamp - Date.now());
  const days = Math.floor(remaining / 86_400_000);
  const hours = Math.floor((remaining % 86_400_000) / 3_600_000);
  const minutes = Math.floor((remaining % 3_600_000) / 60_000);
  return days > 0 ? `${days}d ${hours}h ${minutes}m` : `${hours}h ${minutes}m`;
}

function channelLabel(channel: PushChannelConfig): string {
  return channel.nickname.trim()
    || channelTypeLabel(channel);
}

function channelTypeLabel(channel: PushChannelConfig | PushChannelType): string {
  const channelType = typeof channel === "string" ? channel : channel.channelType;
  return channelOptions.find((item) => item.value === channelType)?.label || channelType;
}

function cloneChannel(channel: PushChannelConfig): PushChannelConfig {
  return JSON.parse(JSON.stringify(channel)) as PushChannelConfig;
}

function cloneSettings(value: PushSettings): PushSettings {
  return JSON.parse(JSON.stringify(value)) as PushSettings;
}

function captureSettingsSnapshot(value: PushSettings): void {
  savedSettingsSnapshot.value = cloneSettings(value);
}

function captureChannelSnapshots(channels: PushChannelConfig[] = settings.channels): void {
  savedChannelSnapshots.value = Object.fromEntries(
    channels.map((channel) => [channel.id, cloneChannel(channel)]),
  );
}

function ensureSelectedChannel(): void {
  if (settings.channels.some((channel) => channel.id === selectedChannelId.value)) return;
  selectedChannelId.value = settings.channels[0]?.id || "";
}

function ruleAccountSummary(rule: PushRule): string {
  const knownAccounts = rule.accountIds
    .map((id) => props.accounts.find((account) => account.id === id))
    .filter((account): account is CodexAccount => Boolean(account));
  const labels = knownAccounts
    .slice(0, 2)
    .map(accountLabel);
  if (!labels.length) {
    return rule.accountIds.length ? `${rule.accountIds.length} ${t("个账号")}` : t("未选择账号");
  }
  return rule.accountIds.length > labels.length
    ? `${labels.join("、")} +${rule.accountIds.length - labels.length}`
    : labels.join("、");
}

function ruleChannels(rule: PushRule): PushChannelConfig[] {
  return rule.channelIds
    .map((id) => settings.channels.find((channel) => channel.id === id))
    .filter((channel): channel is PushChannelConfig => Boolean(channel));
}

function triggerItems(rule: PushRule): string[] {
  const trigger = rule.triggers;
  const items: string[] = [];
  if (trigger.scheduleEnabled) items.push(`${t("定时")} ${durationLabel(trigger.scheduleIntervalMinutes)}`);
  if (trigger.quotaBelowEnabled) items.push(`${t("额度低于")} ${trigger.quotaBelowPercent}%`);
  if (trigger.subscriptionExpiryEnabled) items.push(`${t("订阅剩余")} ≤ ${hoursLabel(trigger.subscriptionExpiryHours)}`);
  if (trigger.tokenExpiryEnabled) items.push(`Token ${t("剩余")} ≤ ${hoursLabel(trigger.tokenExpiryHours)}`);
  if (trigger.tokenExpiredEnabled) items.push(t("Token 已过期"));
  if (trigger.anomalyEnabled) items.push(t("账号异常"));
  return items;
}

function triggerSummary(rule: PushRule): string {
  const items = triggerItems(rule);
  return items.length > 2 ? `${items.slice(0, 2).join(" / ")} +${items.length - 2}` : items.join(" / ") || t("未设置触发条件");
}

function nextRuleTime(rule: PushRule): number {
  if (
    !rule.accountIds.length
    || !ruleChannels(rule).some((channel) => channel.enabled)
    || !hasTrigger(rule)
  ) {
    return 0;
  }
  return [rule.nextRunAt, rule.nextEvaluationAt]
    .filter((value) => Number.isFinite(value) && value > 0)
    .sort((left, right) => left - right)[0] || 0;
}

function durationLabel(minutes: number): string {
  if (minutes % 1440 === 0) return `${minutes / 1440}${t("天")}`;
  if (minutes % 60 === 0) return `${minutes / 60}${t("小时")}`;
  return `${minutes}${t("分钟")}`;
}

function hoursLabel(hours: number): string {
  return hours % 24 === 0 ? `${hours / 24}${t("天")}` : `${hours}${t("小时")}`;
}

function formatTime(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "--";
  return new Date(value).toLocaleString();
}

async function load(): Promise<void> {
  loading.value = true;
  settingsLoaded.value = false;
  const logsPromise = loadLogs();
  try {
    const nextSettings = await getPushSettings();
    Object.assign(settings, defaultPushSettings(), nextSettings, {
      rules: nextSettings.rules || [],
      channels: nextSettings.channels || [],
    });
    sanitizeSelections();
    captureSettingsSnapshot(nextSettings);
    captureChannelSnapshots();
    ensureSelectedChannel();
    settingsLoaded.value = true;
  } catch (error) {
    Message.error(`${t("加载推送设置失败")}：${errorText(error)}`);
  } finally {
    loading.value = false;
    await logsPromise;
  }
}

function sanitizeSelections(): void {
  const channelIds = new Set(settings.channels.map((channel) => channel.id));
  const accountOrder = new Map(props.accounts.map((account, index) => [account.id, index]));
  for (const rule of settings.rules) {
    const accountIds = [...new Set(rule.accountIds.map((id) => id.trim()).filter(Boolean))];
    const originalOrder = new Map(accountIds.map((id, index) => [id, index]));
    rule.accountIds = accountIds.sort((left, right) => {
      const leftOrder = accountOrder.get(left);
      const rightOrder = accountOrder.get(right);
      if (leftOrder !== undefined || rightOrder !== undefined) {
        if (leftOrder === undefined) return 1;
        if (rightOrder === undefined) return -1;
        if (leftOrder !== rightOrder) return leftOrder - rightOrder;
      }
      return (originalOrder.get(left) || 0) - (originalOrder.get(right) || 0);
    });
    rule.channelIds = [...new Set(rule.channelIds.map((id) => id.trim()).filter((id) => channelIds.has(id)))];
  }
}

function addRule(): void {
  const rule = createPushRule(settings.rules.length + 1);
  rule.name = `${t("账号状态提醒")} ${settings.rules.length + 1}`;
  settings.rules.unshift(rule);
  editingRuleId.value = rule.id;
  activeTab.value = "rules";
}

function duplicateRule(rule: PushRule): void {
  const copy = JSON.parse(JSON.stringify(rule)) as PushRule;
  copy.id = globalThis.crypto?.randomUUID?.() || `rule-${Date.now()}`;
  copy.name = `${rule.name} ${t("副本")}`;
  copy.nextRunAt = 0;
  copy.nextEvaluationAt = 0;
  copy.lastSentAt = 0;
  copy.eventLastSentAt = {};
  copy.scheduledRetryDeliveryKeys = [];
  const index = settings.rules.findIndex((item) => item.id === rule.id);
  settings.rules.splice(index + 1, 0, copy);
  editingRuleId.value = copy.id;
}

function removeRule(ruleId: string): void {
  settings.rules = settings.rules.filter((rule) => rule.id !== ruleId);
  if (editingRuleId.value === ruleId) editingRuleId.value = "";
}

function toggleRuleEditor(ruleId: string): void {
  editingRuleId.value = editingRuleId.value === ruleId ? "" : ruleId;
}

function openAccountPicker(rule: PushRule): void {
  accountPickerRuleId.value = rule.id;
  accountPickerSearch.value = "";
  accountPickerSelection.value = new Set(rule.accountIds);
  accountPickerVisible.value = true;
}

function togglePickerAccount(accountId: string, checked: boolean): void {
  const next = new Set(accountPickerSelection.value);
  if (checked) next.add(accountId);
  else next.delete(accountId);
  accountPickerSelection.value = next;
}

function toggleAllPickerAccounts(checked: boolean): void {
  const next = new Set(accountPickerSelection.value);
  for (const account of filteredAccountPickerOptions.value) {
    if (checked) next.add(account.id);
    else next.delete(account.id);
  }
  accountPickerSelection.value = next;
}

function confirmAccountPicker(): void {
  const rule = accountPickerRule.value;
  if (!rule) return;
  rule.accountIds = accountOptions.value
    .filter((account) => accountPickerSelection.value.has(account.id))
    .map((account) => account.id);
  accountPickerVisible.value = false;
}

function hasTrigger(rule: PushRule): boolean {
  return triggerItems(rule).length > 0;
}

function validationIssue(rule?: PushRule): { message: string; tab: "rules" | "channels"; ruleId?: string } | undefined {
  const rules = rule ? [rule] : enabledRules.value;
  for (const current of rules) {
    if (!current.accountIds.length) return { message: t("请选择至少一个账号"), tab: "rules", ruleId: current.id };
    if (current.accountIds.some((accountId) => !accountOptionIds.value.has(accountId))) {
      return { message: t("规则包含已失效账号，请重新选择"), tab: "rules", ruleId: current.id };
    }
    if (!current.channelIds.length) return { message: t("请选择至少一个推送渠道"), tab: "rules", ruleId: current.id };
    if (!hasTrigger(current)) return { message: t("请至少启用一个触发条件"), tab: "rules", ruleId: current.id };
    const channels = ruleChannels(current);
    if (!channels.some((channel) => channel.enabled)) {
      return { message: t("规则选择的渠道均未启用"), tab: "channels", ruleId: current.id };
    }
    for (const channel of channels.filter((channel) => channel.enabled)) {
      const missing = channelMissingField(channel);
      if (missing) return { message: `${channelLabel(channel)}：${t("请填写")} ${missing}`, tab: "channels" };
    }
  }
  return undefined;
}

function channelMissingField(channel: PushChannelConfig): string {
  if (channel.channelType === "serverChan") return channel.serverChanSendKey.trim() ? "" : "SendKey";
  if (channel.channelType === "pushPlus") return channel.pushPlusToken.trim() ? "" : "Token";
  if (channel.channelType === "enterpriseWechat") {
    if (!channel.enterpriseWechatCorpId.trim()) return t("企业 ID");
    if (!channel.enterpriseWechatCorpSecret.trim()) return "Secret";
    return channel.enterpriseWechatAgentId.trim() ? "" : "AgentId";
  }
  if (channel.channelType === "wxPusher") {
    if (!channel.wxPusherAppToken.trim()) return "AppToken";
    return channel.wxPusherUid.trim() ? "" : "UID";
  }
  if (channel.channelType === "bark") return channel.barkToken.trim() ? "" : "Device Key";
  if (channel.channelType === "chanify") return channel.chanifyToken.trim() ? "" : "Sender Token";
  if (channel.channelType === "pushDeer") return channel.pushDeerKey.trim() ? "" : "PushKey";
  return channel.dingTalkAccessToken.trim() ? "" : "AccessToken";
}

function revealValidation(issue: ReturnType<typeof validationIssue>): boolean {
  if (!issue) return false;
  activeTab.value = issue.tab;
  if (issue.ruleId) editingRuleId.value = issue.ruleId;
  Message.warning(issue.message);
  return true;
}

async function persistSettings(options: { silent?: boolean; validateRules?: boolean; successMessage?: string } = {}): Promise<boolean> {
  if (saving.value) return false;
  if (!settingsLoaded.value) {
    Message.warning(t("推送设置尚未加载完成"));
    return false;
  }
  sanitizeSelections();
  if (options.validateRules !== false && revealValidation(validationIssue())) return false;
  saving.value = true;
  try {
    const saved = await updatePushSettings(JSON.parse(JSON.stringify(settings)) as PushSettings);
    Object.assign(settings, saved, { rules: saved.rules || [], channels: saved.channels || [] });
    captureSettingsSnapshot(saved);
    captureChannelSnapshots();
    ensureSelectedChannel();
    if (!options.silent) Message.success(options.successMessage || t("推送设置已保存"));
    return true;
  } catch (error) {
    Message.error(`${t("保存推送设置失败")}：${errorText(error)}`);
    return false;
  } finally {
    saving.value = false;
  }
}

async function save(options: { silent?: boolean } = {}): Promise<boolean> {
  return persistSettings({ ...options, validateRules: true });
}

async function saveRuleForRun(rule: PushRule): Promise<boolean> {
  if (saving.value || !settingsLoaded.value) return false;
  saving.value = true;
  try {
    const payload = cloneSettings(savedSettingsSnapshot.value);
    const nextRule = cloneSettings({
      automationEnabled: settings.automationEnabled,
      rules: [rule],
      channels: [],
    }).rules[0];
    const ruleIndex = payload.rules.findIndex((item) => item.id === rule.id);
    if (ruleIndex >= 0) payload.rules.splice(ruleIndex, 1, nextRule);
    else payload.rules.push(nextRule);
    for (const channel of ruleChannels(rule)) {
      const channelIndex = payload.channels.findIndex((item) => item.id === channel.id);
      if (channelIndex >= 0) payload.channels.splice(channelIndex, 1, cloneChannel(channel));
      else payload.channels.push(cloneChannel(channel));
    }
    const saved = await updatePushSettings(payload);
    const normalizedRule = saved.rules.find((item) => item.id === rule.id);
    if (!normalizedRule) throw new Error(t("保存的规则不存在"));
    const localRuleIndex = settings.rules.findIndex((item) => item.id === rule.id);
    if (localRuleIndex >= 0) settings.rules.splice(localRuleIndex, 1, normalizedRule);
    for (const channelId of normalizedRule.channelIds) {
      const normalizedChannel = saved.channels.find((channel) => channel.id === channelId);
      const localChannelIndex = settings.channels.findIndex((channel) => channel.id === channelId);
      if (normalizedChannel && localChannelIndex >= 0) {
        settings.channels.splice(localChannelIndex, 1, cloneChannel(normalizedChannel));
      }
    }
    captureSettingsSnapshot(saved);
    captureChannelSnapshots(saved.channels);
    return true;
  } catch (error) {
    Message.error(`${t("保存推送设置失败")}：${errorText(error)}`);
    return false;
  } finally {
    saving.value = false;
  }
}

async function run(rule?: PushRule): Promise<void> {
  if (runningRuleId.value) return;
  sanitizeSelections();
  const targetRules = rule ? [rule] : enabledRules.value;
  if (!targetRules.length || (rule && !rule.enabled)) {
    Message.warning(t("请先新增并启用推送规则"));
    return;
  }
  if (revealValidation(rule ? validationIssue(rule) : validationIssue())) return;
  const persisted = rule
    ? await saveRuleForRun(rule)
    : await save({ silent: true });
  if (!persisted) return;
  runningRuleId.value = rule?.id || "all";
  try {
    const summary = rule ? await runPushRuleNow(rule.id) : await runPushNow();
    showRunSummary(summary);
    if (targetRules.some((item) => item.activeRefresh)) emit("accounts-refreshed");
    await loadLogs();
  } catch (error) {
    Message.error(`${t("立即推送失败")}：${errorText(error)}`);
  } finally {
    runningRuleId.value = "";
  }
}

function showRunSummary(summary: PushRunSummary): void {
  const message = `${t("规则")} ${summary.attemptedRules} · ${t("匹配账号")} ${summary.matchedAccounts} · ${t("成功")} ${summary.successfulDeliveries} · ${t("失败")} ${summary.failedDeliveries}`;
  if (summary.failedDeliveries) Message.warning(message);
  else if (!summary.matchedAccounts) Message.info(`${message} · ${t("当前没有账号满足条件")}`);
  else Message.success(message);
}

function addChannel(value: string | number | Record<string, unknown> | undefined): void {
  if (typeof value !== "string" || !channelOptions.some((option) => option.value === value)) return;
  const channel = createPushChannel(value as PushChannelType);
  settings.channels.push(channel);
  selectedChannelId.value = channel.id;
  channelSearch.value = "";
  activeTab.value = "channels";
}

function removeChannel(channelId: string): void {
  const index = settings.channels.findIndex((channel) => channel.id === channelId);
  const nextSelectedId = settings.channels[index + 1]?.id || settings.channels[index - 1]?.id || "";
  settings.channels = settings.channels.filter((channel) => channel.id !== channelId);
  for (const rule of settings.rules) rule.channelIds = rule.channelIds.filter((id) => id !== channelId);
  const nextSnapshots = { ...savedChannelSnapshots.value };
  delete nextSnapshots[channelId];
  savedChannelSnapshots.value = nextSnapshots;
  if (selectedChannelId.value === channelId) selectedChannelId.value = nextSelectedId;
}

function cancelSelectedChannel(): void {
  const channel = selectedChannel.value;
  if (!channel) return;
  const snapshot = savedChannelSnapshots.value[channel.id];
  if (!snapshot) {
    removeChannel(channel.id);
    return;
  }
  const index = settings.channels.findIndex((item) => item.id === channel.id);
  settings.channels.splice(index, 1, cloneChannel(snapshot));
}

async function saveSelectedChannel(): Promise<void> {
  const channel = selectedChannel.value;
  if (!channel) return;
  const missing = channel.enabled ? channelMissingField(channel) : "";
  if (missing) {
    Message.warning(`${channelLabel(channel)}：${t("请填写")} ${missing}`);
    return;
  }
  if (saving.value) return;
  if (!settingsLoaded.value) {
    Message.warning(t("推送设置尚未加载完成"));
    return;
  }
  saving.value = true;
  try {
    const payload = cloneSettings(savedSettingsSnapshot.value);
    const channelIndex = payload.channels.findIndex((item) => item.id === channel.id);
    if (channelIndex >= 0) payload.channels.splice(channelIndex, 1, cloneChannel(channel));
    else payload.channels.push(cloneChannel(channel));
    const saved = await updatePushSettings(payload);
    const normalizedChannel = saved.channels.find((item) => item.id === channel.id);
    if (!normalizedChannel) throw new Error(t("保存的渠道不存在"));
    const currentIndex = settings.channels.findIndex((item) => item.id === channel.id);
    if (currentIndex >= 0) settings.channels.splice(currentIndex, 1, cloneChannel(normalizedChannel));
    captureSettingsSnapshot(saved);
    captureChannelSnapshots(saved.channels);
    Message.success(t("渠道已保存"));
  } catch (error) {
    Message.error(`${t("保存推送设置失败")}：${errorText(error)}`);
  } finally {
    saving.value = false;
  }
}

async function testChannel(channel: PushChannelConfig): Promise<void> {
  if (testingChannelId.value) return;
  const missing = channelMissingField(channel);
  if (missing) {
    Message.warning(`${channelLabel(channel)}：${t("请填写")} ${missing}`);
    return;
  }
  testingChannelId.value = channel.id;
  try {
    const result = await testPushChannel({ ...channel });
    result.success
      ? Message.success(`${result.channelName}：${t("测试推送成功")}`)
      : Message.error(`${result.channelName}：${result.message}`);
    await loadLogs();
  } catch (error) {
    Message.error(`${t("测试推送失败")}：${errorText(error)}`);
  } finally {
    testingChannelId.value = "";
  }
}

async function loadLogs(): Promise<void> {
  logsLoading.value = true;
  try {
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    const [nextLogs, successfulCount] = await Promise.all([
      listPushLogs(),
      countSuccessfulPushLogsSince(start.getTime()),
    ]);
    logs.value = nextLogs;
    todayDeliveryCount.value = successfulCount;
  } catch (error) {
    Message.error(`${t("加载推送日志失败")}：${errorText(error)}`);
  } finally {
    logsLoading.value = false;
  }
}

async function clearLogs(): Promise<void> {
  try {
    await clearPushLogs();
    logs.value = [];
    todayDeliveryCount.value = 0;
    Message.success(t("推送日志已清空"));
  } catch (error) {
    Message.error(`${t("清空推送日志失败")}：${errorText(error)}`);
  }
}

function eventLabel(value: string): string {
  const labels: Record<string, string> = {
    schedule: t("定时状态"),
    quotaBelow: t("额度不足"),
    subscriptionExpiry: t("订阅临期"),
    tokenExpirySoon: t("Token 临期"),
    tokenExpired: t("Token 已过期"),
    anomaly: t("账号异常"),
    test: t("测试"),
  };
  return value.split(",").map((event) => labels[event] || event).join(" / ");
}

function triggerLabel(value: string): string {
  if (value === "scheduled") return t("自动");
  if (value === "manual") return t("手动");
  if (value === "test") return t("测试");
  return value;
}
</script>

<template>
  <section class="push-settings-page">
    <header class="push-page-header">
      <div class="push-page-title">
        <a-button type="text" class="push-back-button" :title="t('返回设置')" @click="emit('back')">
          <template #icon><icon-left /></template>
        </a-button>
        <div>
          <h2>{{ t("推送设置") }}</h2>
          <span>{{ t("通过规则将账号状态推送到一个或多个渠道") }}</span>
        </div>
      </div>
      <div class="push-page-actions">
        <label class="push-automation-toggle">
          <span>{{ t("自动执行") }}</span>
          <a-switch v-model="settings.automationEnabled" size="small" :disabled="!settingsLoaded" />
        </label>
        <a-button :loading="runningRuleId === 'all'" :disabled="!settingsLoaded" @click="run()">
          <template #icon><icon-send /></template>
          {{ t("执行全部") }}
        </a-button>
        <a-button type="primary" :loading="saving" :disabled="!settingsLoaded" @click="save()">
          <template #icon><icon-save /></template>
          {{ t("保存设置") }}
        </a-button>
      </div>
    </header>

    <a-spin :loading="loading" dot>
      <section class="push-overview-grid">
        <article><span class="push-metric-icon blue"><icon-list /></span><div><small>{{ t("推送规则") }}</small><strong>{{ settings.rules.length }}</strong></div></article>
        <article><span class="push-metric-icon green"><icon-check-circle /></span><div><small>{{ t("已启用规则") }}</small><strong>{{ enabledRules.length }}</strong></div></article>
        <article><span class="push-metric-icon violet"><icon-notification /></span><div><small>{{ t("推送渠道") }}</small><strong>{{ enabledChannels.length }}</strong></div></article>
        <article><span class="push-metric-icon orange"><icon-send /></span><div><small>{{ t("今日发送") }}</small><strong>{{ todayDeliveryCount }}</strong></div></article>
      </section>

      <a-tabs v-model:active-key="activeTab" class="push-tabs" type="line">
        <a-tab-pane key="rules" :title="t('推送规则')">
          <div class="push-section-toolbar">
            <div class="push-rule-toolbar-copy">
              <strong>{{ t("规则列表") }}</strong>
              <span>{{ t("账号仅在规则编辑时选择，不在页面中铺开显示") }}</span>
            </div>
            <a-button type="primary" :disabled="!settingsLoaded" @click="addRule">
              <template #icon><icon-plus /></template>
              {{ t("新增规则") }}
            </a-button>
          </div>

          <div v-if="settings.rules.length" class="push-rule-table">
            <div class="push-rule-head">
              <span>{{ t("规则名称") }}</span><span>{{ t("账号范围") }}</span><span>{{ t("推送渠道") }}</span>
              <span>{{ t("触发条件") }}</span><span>{{ t("下次检查") }}</span><span>{{ t("启用") }}</span><span>{{ t("操作") }}</span>
            </div>
            <template v-for="rule in settings.rules" :key="rule.id">
              <div class="push-rule-row" :class="{ expanded: editingRuleId === rule.id }">
                <button class="push-rule-name" type="button" @click="toggleRuleEditor(rule.id)">
                  <strong>{{ rule.name }}</strong>
                  <span>{{ triggerItems(rule).length }} {{ t("个触发器") }}</span>
                </button>
                <span class="push-rule-summary" :title="ruleAccountSummary(rule)">{{ ruleAccountSummary(rule) }}</span>
                <div class="push-rule-channel-tags">
                  <a-tag v-for="channel in ruleChannels(rule).slice(0, 2)" :key="channel.id" color="arcoblue">{{ channelLabel(channel) }}</a-tag>
                  <small v-if="rule.channelIds.length > 2">+{{ rule.channelIds.length - 2 }}</small>
                  <span v-if="!rule.channelIds.length">--</span>
                </div>
                <span class="push-rule-summary" :title="triggerItems(rule).join(' / ')">{{ triggerSummary(rule) }}</span>
                <span class="push-rule-next">{{ rule.enabled && settings.automationEnabled ? formatTime(nextRuleTime(rule)) : "--" }}</span>
                <a-switch v-model="rule.enabled" size="small" />
                <div class="push-rule-actions">
                  <a-button size="mini" :title="t('执行规则')" :loading="runningRuleId === rule.id" :disabled="!rule.enabled" @click="run(rule)"><template #icon><icon-play-arrow /></template></a-button>
                  <a-button size="mini" :title="t('编辑')" @click="toggleRuleEditor(rule.id)"><template #icon><icon-edit /></template></a-button>
                  <a-button size="mini" :title="t('复制')" @click="duplicateRule(rule)"><template #icon><icon-copy /></template></a-button>
                  <a-popconfirm :content="t('确认删除这个推送规则？')" :ok-text="t('确认')" :cancel-text="t('取消')" @ok="removeRule(rule.id)">
                    <a-button size="mini" status="danger" :title="t('删除')"><template #icon><icon-delete /></template></a-button>
                  </a-popconfirm>
                </div>
              </div>

              <section v-if="editingRuleId === rule.id" class="push-rule-editor">
                <div class="push-rule-form-grid">
                  <label><span>{{ t("规则名称") }}</span><a-input v-model="rule.name" :placeholder="t('例如：重要账号额度预警')" /></label>
                  <label>
                    <span>{{ t("账号范围") }} <em>{{ rule.accountIds.length }} {{ t("个账号") }}</em></span>
                    <button type="button" class="push-account-picker-trigger" @click="openAccountPicker(rule)">
                      <span :class="{ placeholder: !rule.accountIds.length }">{{ rule.accountIds.length ? ruleAccountSummary(rule) : t("选择要监控的账号") }}</span>
                      <icon-search />
                    </button>
                  </label>
                  <label>
                    <span>{{ t("推送渠道") }} <em>{{ rule.channelIds.length }} {{ t("个渠道") }}</em></span>
                    <a-select v-model="rule.channelIds" multiple allow-clear :placeholder="t('选择一个或多个渠道')" popup-container="body">
                      <a-option v-for="channel in settings.channels" :key="channel.id" :value="channel.id" :disabled="!channel.enabled">{{ channelLabel(channel) }}</a-option>
                    </a-select>
                  </label>
                </div>

                <div class="push-editor-section-title">
                  <div><strong>{{ t("触发条件") }}</strong><span>{{ t("可同时启用多个条件，任一条件满足即可推送") }}</span></div>
                </div>
                <div class="push-trigger-grid">
                  <article :class="{ active: rule.triggers.scheduleEnabled }">
                    <header><div><icon-clock-circle /><strong>{{ t("定时推送") }}</strong></div><a-switch v-model="rule.triggers.scheduleEnabled" size="small" /></header>
                    <label><span>{{ t("每隔") }}</span><a-input-number v-model="rule.triggers.scheduleIntervalMinutes" :min="1" :max="43200" :disabled="!rule.triggers.scheduleEnabled"><template #suffix>{{ t("分钟") }}</template></a-input-number></label>
                  </article>
                  <article :class="{ active: rule.triggers.quotaBelowEnabled }">
                    <header><div><icon-dashboard /><strong>{{ t("额度不足") }}</strong></div><a-switch v-model="rule.triggers.quotaBelowEnabled" size="small" /></header>
                    <label><span>{{ t("剩余额度低于") }}</span><a-input-number v-model="rule.triggers.quotaBelowPercent" :min="0" :max="100" :disabled="!rule.triggers.quotaBelowEnabled"><template #suffix>%</template></a-input-number></label>
                  </article>
                  <article :class="{ active: rule.triggers.subscriptionExpiryEnabled }">
                    <header><div><icon-calendar /><strong>{{ t("订阅临期") }}</strong></div><a-switch v-model="rule.triggers.subscriptionExpiryEnabled" size="small" /></header>
                    <label><span>{{ t("剩余时间不超过") }}</span><a-input-number v-model="rule.triggers.subscriptionExpiryHours" :min="1" :max="8760" :disabled="!rule.triggers.subscriptionExpiryEnabled"><template #suffix>{{ t("小时") }}</template></a-input-number></label>
                  </article>
                  <article :class="{ active: rule.triggers.tokenExpiryEnabled }">
                    <header><div><icon-safe /><strong>{{ t("Token 临期") }}</strong></div><a-switch v-model="rule.triggers.tokenExpiryEnabled" size="small" /></header>
                    <label><span>{{ t("剩余时间不超过") }}</span><a-input-number v-model="rule.triggers.tokenExpiryHours" :min="1" :max="8760" :disabled="!rule.triggers.tokenExpiryEnabled"><template #suffix>{{ t("小时") }}</template></a-input-number></label>
                  </article>
                  <article :class="{ active: rule.triggers.tokenExpiredEnabled }">
                    <header><div><icon-exclamation-circle /><strong>{{ t("Token 已过期") }}</strong></div><a-switch v-model="rule.triggers.tokenExpiredEnabled" size="small" /></header>
                    <p>{{ t("检测到 Token 已失效或接口返回过期状态时推送") }}</p>
                  </article>
                  <article :class="{ active: rule.triggers.anomalyEnabled }">
                    <header><div><icon-bug /><strong>{{ t("账号异常") }}</strong></div><a-switch v-model="rule.triggers.anomalyEnabled" size="small" /></header>
                    <p>{{ t("读取额度发生错误或账号状态异常时推送") }}</p>
                  </article>
                </div>

                <div class="push-rule-advanced">
                  <label><span>{{ t("结果排序") }}</span><a-select v-model="rule.sortBy" popup-container="body"><a-option value="accountOrder">{{ t("账号列表顺序") }}</a-option><a-option value="quotaAsc">{{ t("剩余额度升序") }}</a-option><a-option value="subscriptionExpiryAsc">{{ t("订阅到期升序") }}</a-option><a-option value="tokenExpiryAsc">{{ t("Token 到期升序") }}</a-option></a-select></label>
                  <label><span>{{ t("重复提醒间隔") }}</span><a-input-number v-model="rule.cooldownMinutes" :min="1" :max="10080"><template #suffix>{{ t("分钟") }}</template></a-input-number></label>
                  <label class="push-inline-switch"><div><span>{{ t("推送前主动刷新") }}</span><small>{{ t("关闭时直接读取定时任务保存的账号状态") }}</small></div><a-switch v-model="rule.activeRefresh" size="small" /></label>
                </div>
                <div class="push-rule-preview">
                  <div><strong>{{ t("规则预览") }}</strong><span>{{ ruleAccountSummary(rule) }} → {{ ruleChannels(rule).map(channelLabel).join("、") || t("未选择渠道") }}</span></div>
                  <div class="push-preview-triggers"><a-tag v-for="item in triggerItems(rule)" :key="item">{{ item }}</a-tag></div>
                </div>
              </section>
            </template>
          </div>
          <a-empty v-else :description="t('还没有推送规则')"><a-button type="primary" @click="addRule">{{ t("新增第一条规则") }}</a-button></a-empty>
        </a-tab-pane>

        <a-tab-pane key="channels" :title="t('推送渠道')">
          <div class="push-section-toolbar channel-toolbar">
            <div class="push-rule-toolbar-copy"><strong>{{ t("渠道管理") }}</strong><span>{{ t("规则可同时选择多个已启用渠道") }}</span></div>
            <a-dropdown trigger="click" position="bl" popup-container="body" :popup-max-height="false" @select="addChannel">
              <a-button type="primary" :disabled="!settingsLoaded">
                <template #icon><icon-plus /></template>
                {{ t("添加渠道") }} <icon-down />
              </a-button>
              <template #content>
                <a-doption v-for="option in channelOptions" :key="option.value" :value="option.value">
                  {{ option.label }}
                </a-doption>
              </template>
            </a-dropdown>
          </div>
          <div v-if="settings.channels.length" class="push-channel-workspace">
            <aside class="push-channel-sidebar">
              <a-input v-model="channelSearch" allow-clear :placeholder="t('搜索渠道')">
                <template #prefix><icon-search /></template>
              </a-input>
              <div class="push-channel-nav-list">
                <button
                  v-for="channel in filteredChannels"
                  :key="channel.id"
                  type="button"
                  class="push-channel-nav-item"
                  :class="{ active: selectedChannelId === channel.id }"
                  @click="selectedChannelId = channel.id"
                >
                  <span class="push-channel-brand-icon" :class="channel.channelType">
                    <icon-message v-if="channel.channelType === 'enterpriseWechat' || channel.channelType === 'wxPusher'" />
                    <icon-notification v-else-if="channel.channelType === 'bark' || channel.channelType === 'chanify' || channel.channelType === 'pushDeer'" />
                    <icon-send v-else />
                  </span>
                  <span class="push-channel-nav-copy">
                    <strong>{{ channelTypeLabel(channel) }}</strong>
                    <small>{{ channel.nickname.trim() || t("未设置渠道昵称") }}</small>
                  </span>
                  <span class="push-channel-status" :class="{ enabled: channel.enabled }">
                    <i></i>{{ t(channel.enabled ? "启用" : "未启用") }}
                  </span>
                  <icon-right />
                </button>
                <a-empty v-if="!filteredChannels.length" :description="t('没有匹配的渠道')" />
              </div>
            </aside>

            <section v-if="selectedChannel" class="push-channel-detail">
              <header class="push-channel-detail-header">
                <div class="push-channel-detail-title">
                  <span class="push-channel-brand-icon large" :class="selectedChannel.channelType">
                    <icon-message v-if="selectedChannel.channelType === 'enterpriseWechat' || selectedChannel.channelType === 'wxPusher'" />
                    <icon-notification v-else-if="selectedChannel.channelType === 'bark' || selectedChannel.channelType === 'chanify' || selectedChannel.channelType === 'pushDeer'" />
                    <icon-send v-else />
                  </span>
                  <div><strong>{{ channelTypeLabel(selectedChannel) }}</strong><span>{{ selectedChannel.nickname.trim() || t("未设置渠道昵称") }}</span></div>
                </div>
                <div class="push-channel-detail-actions">
                  <label><a-switch v-model="selectedChannel.enabled" size="small" /><span>{{ t(selectedChannel.enabled ? "启用" : "未启用") }}</span></label>
                  <a-button :loading="testingChannelId === selectedChannel.id" :disabled="Boolean(testingChannelId && testingChannelId !== selectedChannel.id)" @click="testChannel(selectedChannel)"><template #icon><icon-send /></template>{{ t("测试") }}</a-button>
                  <a-popconfirm :content="t('确认删除这个推送渠道？')" :ok-text="t('确认')" :cancel-text="t('取消')" @ok="removeChannel(selectedChannel.id)"><a-button status="danger" :title="t('删除')"><template #icon><icon-delete /></template></a-button></a-popconfirm>
                </div>
              </header>

              <div class="push-channel-detail-body">
                <div class="push-channel-fields">
                  <label><span>{{ t("渠道昵称") }}</span><a-input v-model="selectedChannel.nickname" allow-clear :placeholder="t('渠道昵称')" /></label>
                  <template v-if="selectedChannel.channelType === 'serverChan'"><label><span>SendKey</span><a-input-password v-model="selectedChannel.serverChanSendKey" /></label></template>
                  <template v-else-if="selectedChannel.channelType === 'pushPlus'"><label><span>Token</span><a-input-password v-model="selectedChannel.pushPlusToken" /></label><label><span>Topic</span><a-input v-model="selectedChannel.pushPlusTopic" /></label></template>
                  <template v-else-if="selectedChannel.channelType === 'enterpriseWechat'"><label><span>AgentId</span><a-input v-model="selectedChannel.enterpriseWechatAgentId" /></label><label><span>{{ t("企业 ID") }}</span><a-input v-model="selectedChannel.enterpriseWechatCorpId" /></label><label><span>ToUser</span><a-input v-model="selectedChannel.enterpriseWechatToUser" /></label><label><span>Secret</span><a-input-password v-model="selectedChannel.enterpriseWechatCorpSecret" /></label></template>
                  <template v-else-if="selectedChannel.channelType === 'wxPusher'"><label><span>AppToken</span><a-input-password v-model="selectedChannel.wxPusherAppToken" /></label><label><span>UID</span><a-input v-model="selectedChannel.wxPusherUid" /></label></template>
                  <template v-else-if="selectedChannel.channelType === 'bark'"><label><span>API</span><a-input v-model="selectedChannel.barkApi" /></label><label><span>Device Key</span><a-input-password v-model="selectedChannel.barkToken" /></label><label><span>Sound</span><a-input v-model="selectedChannel.barkSound" /></label></template>
                  <template v-else-if="selectedChannel.channelType === 'chanify'"><label><span>Sender Token</span><a-input-password v-model="selectedChannel.chanifyToken" /></label></template>
                  <template v-else-if="selectedChannel.channelType === 'pushDeer'"><label><span>PushKey</span><a-input-password v-model="selectedChannel.pushDeerKey" /></label></template>
                  <template v-else><label><span>AccessToken</span><a-input-password v-model="selectedChannel.dingTalkAccessToken" /></label><label><span>Secret</span><a-input-password v-model="selectedChannel.dingTalkSecret" /></label></template>
                </div>
                <div class="push-channel-note"><icon-info-circle />{{ t("规则可同时选择多个已启用渠道") }}</div>
                <footer class="push-channel-detail-footer">
                  <a-button @click="cancelSelectedChannel">{{ t("取消") }}</a-button>
                  <a-button type="primary" :loading="saving" :disabled="!settingsLoaded" @click="saveSelectedChannel"><template #icon><icon-save /></template>{{ t("保存渠道") }}</a-button>
                </footer>
              </div>
            </section>
          </div>
          <a-empty v-else :description="t('还没有推送渠道')" />
        </a-tab-pane>

        <a-tab-pane key="logs" :title="t('推送日志')">
          <div class="push-section-toolbar log-toolbar"><div class="push-rule-toolbar-copy"><strong>{{ t("推送日志") }}</strong><span>{{ t("最近") }} {{ logs.length }} {{ t("条记录") }}</span></div><div><a-button :loading="logsLoading" @click="loadLogs"><template #icon><icon-refresh /></template>{{ t("刷新") }}</a-button><a-popconfirm :content="t('确认清空全部推送日志？')" :ok-text="t('确认')" :cancel-text="t('取消')" @ok="clearLogs"><a-button status="danger" :disabled="!logs.length"><template #icon><icon-delete /></template>{{ t("清空") }}</a-button></a-popconfirm></div></div>
          <a-spin :loading="logsLoading" dot>
            <div v-if="logs.length" class="push-log-table">
              <div class="push-log-head"><span>{{ t("时间") }}</span><span>{{ t("规则 / 触发") }}</span><span>{{ t("匹配账号 / 事件") }}</span><span>{{ t("渠道") }}</span><span>{{ t("结果") }}</span><span>{{ t("内容") }}</span><span>{{ t("响应") }}</span></div>
              <div v-for="log in logs" :key="log.id" class="push-log-row"><span>{{ formatTime(log.createdAt) }}</span><div><strong>{{ log.ruleName || t("渠道测试") }}</strong><span>{{ triggerLabel(log.trigger) }}</span></div><div><strong>{{ privacyLogAccountLabel(log.accountLabel || "--") }}</strong><span>{{ eventLabel(log.eventTypes) }}</span></div><span>{{ log.channelName }}</span><a-tag :color="log.success ? 'green' : 'red'">{{ t(log.success ? "成功" : "失败") }}</a-tag><span class="push-log-response" :title="privacyLogContent(log.content)">{{ privacyLogContent(log.content) }}</span><span class="push-log-response" :title="privacyLogContent(log.response)">{{ privacyLogContent(log.response) }}</span></div>
            </div>
            <a-empty v-else :description="t('还没有推送日志')" />
          </a-spin>
        </a-tab-pane>
      </a-tabs>
    </a-spin>

    <a-modal
      v-model:visible="accountPickerVisible"
      :title="t('选择要监控的账号')"
      width="820px"
      :footer="false"
      modal-class="push-account-picker-modal"
    >
      <div class="api-service-account-modal push-account-picker-body">
        <p>{{ accountPickerRule?.name || t("账号状态提醒") }}</p>
        <a-input v-model="accountPickerSearch" allow-clear :placeholder="t('筛选邮箱 / 昵称')">
          <template #prefix><icon-search /></template>
        </a-input>
        <div v-if="accountOptions.length" class="api-service-account-select-all">
          <a-checkbox
            :model-value="allVisiblePickerAccountsSelected"
            :indeterminate="pickerSelectionIndeterminate"
            @change="(checked) => toggleAllPickerAccounts(Boolean(checked))"
          >
            {{ t("全选") }}
          </a-checkbox>
          <span>
            {{ t("已选") }} {{ selectedAccountPickerCount }} / {{ accountOptions.length }}
            <template v-if="accountPickerSearch.trim()">· {{ filteredAccountPickerOptions.length }} {{ t("条") }}</template>
          </span>
        </div>
        <div class="api-service-account-list push-account-picker-list">
          <label v-for="account in filteredAccountPickerOptions" :key="account.id" class="api-service-account-row">
            <a-checkbox
              :model-value="accountPickerSelection.has(account.id)"
              @change="(checked) => togglePickerAccount(account.id, Boolean(checked))"
            />
            <div class="api-service-account-main">
              <strong>{{ accountLabel(account) }}</strong>
              <span>{{ accountSecondaryLabel(account) }}</span>
              <div
                v-if="accountStatusSource(account).quota && hasAnyQuotaWindow(accountStatusSource(account).quota)"
                class="api-service-account-quota"
              >
                <div v-if="hasQuotaWindow(accountStatusSource(account).quota, 'hourly')" class="api-service-quota-line">
                  <span><icon-clock-circle /> {{ isFreePlanAccount(accountStatusSource(account)) ? t("长周期") : t("短周期") }}</span>
                  <strong :style="{ color: quotaColor(accountStatusSource(account).quota?.hourly_percentage) }">{{ accountStatusSource(account).quota?.hourly_percentage }}%</strong>
                  <small>{{ quotaWindowLabel(accountStatusSource(account).quota?.hourly_window_minutes, 300) }}</small>
                  <em>{{ quotaResetLeftLabel(accountStatusSource(account).quota?.hourly_reset_time) }}</em>
                </div>
                <div
                  v-if="!isFreePlanAccount(accountStatusSource(account)) && hasQuotaWindow(accountStatusSource(account).quota, 'weekly')"
                  class="api-service-quota-line"
                >
                  <span><icon-calendar /> {{ t("长周期") }}</span>
                  <strong :style="{ color: quotaColor(accountStatusSource(account).quota?.weekly_percentage) }">{{ accountStatusSource(account).quota?.weekly_percentage }}%</strong>
                  <small>{{ quotaWindowLabel(accountStatusSource(account).quota?.weekly_window_minutes, 10_080) }}</small>
                  <em>{{ quotaResetLeftLabel(accountStatusSource(account).quota?.weekly_reset_time) }}</em>
                </div>
              </div>
              <div v-else-if="accountStatusSource(account).quota_error" class="api-service-account-quota-error">
                {{ accountStatusSource(account).quota_error?.message }}
              </div>
            </div>
            <PlanBadge :label="planLabel(account)" :badge-class="planClass(account)" />
          </label>
          <a-empty
            v-if="!filteredAccountPickerOptions.length"
            :description="t(accountOptions.length ? '没有匹配的账号' : '暂无可选账号')"
          />
        </div>
        <div class="api-service-modal-actions">
          <a-button @click="accountPickerVisible = false">{{ t("取消") }}</a-button>
          <a-button type="primary" @click="confirmAccountPicker">{{ t("确认") }}</a-button>
        </div>
      </div>
    </a-modal>
  </section>
</template>
