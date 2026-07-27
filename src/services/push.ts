import { invoke } from "@tauri-apps/api/core";

export type PushChannelType =
  | "serverChan"
  | "pushPlus"
  | "enterpriseWechat"
  | "wxPusher"
  | "bark"
  | "chanify"
  | "pushDeer"
  | "dingTalk";

export type PushRuleSortBy =
  | "accountOrder"
  | "quotaAsc"
  | "subscriptionExpiryAsc"
  | "tokenExpiryAsc";

export interface PushChannelConfig {
  id: string;
  channelType: PushChannelType;
  nickname: string;
  enabled: boolean;
  serverChanSendKey: string;
  pushPlusToken: string;
  pushPlusTopic: string;
  enterpriseWechatCorpId: string;
  enterpriseWechatCorpSecret: string;
  enterpriseWechatAgentId: string;
  enterpriseWechatToUser: string;
  wxPusherAppToken: string;
  wxPusherUid: string;
  barkApi: string;
  barkToken: string;
  barkSound: string;
  chanifyToken: string;
  pushDeerKey: string;
  dingTalkAccessToken: string;
  dingTalkSecret: string;
}

export interface PushRuleTriggers {
  scheduleEnabled: boolean;
  scheduleIntervalMinutes: number;
  quotaBelowEnabled: boolean;
  quotaBelowPercent: number;
  subscriptionExpiryEnabled: boolean;
  subscriptionExpiryHours: number;
  tokenExpiryEnabled: boolean;
  tokenExpiryHours: number;
  tokenExpiredEnabled: boolean;
  anomalyEnabled: boolean;
}

export interface PushRule {
  id: string;
  name: string;
  enabled: boolean;
  accountIds: string[];
  channelIds: string[];
  triggers: PushRuleTriggers;
  sortBy: PushRuleSortBy;
  activeRefresh: boolean;
  cooldownMinutes: number;
  nextRunAt: number;
  nextEvaluationAt: number;
  lastSentAt: number;
  eventLastSentAt: Record<string, number>;
  scheduledRetryDeliveryKeys: string[];
}

export interface PushSettings {
  automationEnabled: boolean;
  rules: PushRule[];
  channels: PushChannelConfig[];
}

export interface PushLogEntry {
  id: number;
  createdAt: number;
  trigger: "scheduled" | "manual" | "test" | string;
  ruleId?: string | null;
  ruleName?: string | null;
  accountId?: string | null;
  accountLabel?: string | null;
  eventTypes: string;
  channelId: string;
  channelName: string;
  success: boolean;
  title: string;
  content: string;
  response: string;
}

export interface PushRunSummary {
  trigger: string;
  attemptedRules: number;
  matchedAccounts: number;
  attemptedAccounts: number;
  skippedAccounts: number;
  refreshedAccounts: number;
  successfulDeliveries: number;
  failedDeliveries: number;
}

export interface PushChannelTestResult {
  channelId: string;
  channelName: string;
  success: boolean;
  message: string;
}

function createId(prefix: string): string {
  return globalThis.crypto?.randomUUID?.() || `${prefix}-${Date.now()}-${Math.random()}`;
}

export function defaultPushSettings(): PushSettings {
  return {
    automationEnabled: true,
    rules: [],
    channels: [],
  };
}

export function createPushRule(index = 1): PushRule {
  return {
    id: createId("rule"),
    name: `${titledRuleName()} ${index}`,
    enabled: true,
    accountIds: [],
    channelIds: [],
    triggers: {
      scheduleEnabled: true,
      scheduleIntervalMinutes: 1440,
      quotaBelowEnabled: false,
      quotaBelowPercent: 20,
      subscriptionExpiryEnabled: false,
      subscriptionExpiryHours: 168,
      tokenExpiryEnabled: false,
      tokenExpiryHours: 72,
      tokenExpiredEnabled: true,
      anomalyEnabled: true,
    },
    sortBy: "tokenExpiryAsc",
    activeRefresh: false,
    cooldownMinutes: 60,
    nextRunAt: 0,
    nextEvaluationAt: 0,
    lastSentAt: 0,
    eventLastSentAt: {},
    scheduledRetryDeliveryKeys: [],
  };
}

function titledRuleName(): string {
  return "账号状态提醒";
}

export function createPushChannel(channelType: PushChannelType): PushChannelConfig {
  return {
    id: createId("channel"),
    channelType,
    nickname: "",
    enabled: true,
    serverChanSendKey: "",
    pushPlusToken: "",
    pushPlusTopic: "",
    enterpriseWechatCorpId: "",
    enterpriseWechatCorpSecret: "",
    enterpriseWechatAgentId: "",
    enterpriseWechatToUser: "@all",
    wxPusherAppToken: "",
    wxPusherUid: "",
    barkApi: "https://api.day.app",
    barkToken: "",
    barkSound: "",
    chanifyToken: "",
    pushDeerKey: "",
    dingTalkAccessToken: "",
    dingTalkSecret: "",
  };
}

export function getPushSettings(): Promise<PushSettings> {
  return invoke("push_get_settings");
}

export function updatePushSettings(settings: PushSettings): Promise<PushSettings> {
  return invoke("push_update_settings", { settings });
}

export function runPushNow(): Promise<PushRunSummary> {
  return invoke("push_run_now");
}

export function runPushRuleNow(ruleId: string): Promise<PushRunSummary> {
  return invoke("push_run_rule_now", { ruleId });
}

export function testPushChannel(channel: PushChannelConfig): Promise<PushChannelTestResult> {
  return invoke("push_test_channel", { channel });
}

export function listPushLogs(limit = 200): Promise<PushLogEntry[]> {
  return invoke("push_list_logs", { limit });
}

export function countSuccessfulPushLogsSince(startAt: number): Promise<number> {
  return invoke("push_count_successful_logs_since", { startAt });
}

export function clearPushLogs(): Promise<number> {
  return invoke("push_clear_logs");
}
