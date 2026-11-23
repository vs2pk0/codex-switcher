<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import { formatLocalizedCount, t } from "../i18n";
import { formatWallTime } from "../timeZone";
import { accountGridLayout } from "../services/accountLayout";
import { openExternalUrl, type CodexSwitcherSettings } from "../services/codex";
import {
  callPlatform,
  type InstalledExtension,
  type PlatformAccount,
  type PlatformLogin,
  type PlatformQuota,
  type PlatformSwitchResult,
} from "../services/extensions";
import PhoneModal from "./PhoneModal.vue";
import PlanBadge from "./PlanBadge.vue";
import PlatformPicker from "./PlatformPicker.vue";
import PlatformQuotaList from "./PlatformQuotaList.vue";

const props = defineProps<{
  extensions: InstalledExtension[];
  displayText?: (value: string) => string;
  settings: Pick<CodexSwitcherSettings, "maxColumns" | "badgeStyle" | "badgeStyles">;
  viewportWidth: number;
  expandedLayout: boolean;
  statusClockMs: number;
  downloadExport: (content: string, filename: string) => Promise<void>;
}>();

const emit = defineEmits<{
  (event: "toggle-privacy"): void;
  (event: "open-badge-style"): void;
}>();

const choices = computed(() => props.extensions.flatMap((extension) =>
  extension.platforms.map((platform) => ({
    ...platform,
    extensionId: extension.id,
    key: `${extension.id}:${platform.id}`,
    revision: extension.version,
    accountActions: extension.accountActions === true,
    iconUrl: platform.iconPng ? `data:image/png;base64,${platform.iconPng}` : null,
  })),
));
type Choice = (typeof choices.value)[number];
const selection = ref("");
const platform = computed(() => choices.value.find(({ key }) => key === selection.value));
const readChoices = computed(() => choices.value.filter(({ localImport }) => localImport));
const saved = ref(new Map<string, { revision: string; rows: PlatformAccount[] }>());
const loading = ref(false);
const updating = ref<{ key: string; action: string } | null>(null);
const readingCurrent = ref(false);
const busy = computed(() => updating.value !== null || readingCurrent.value);
const lastRead = ref(new Map<string, { revision: string; ids: Set<string> }>());
const editing = ref<{ target: Choice; account: PlatformAccount; kind: "edit" | "phone" } | null>(null);
const quotaDetailsKey = ref<string | null>(null);
const quotaScroll = ref<HTMLElement | null>(null);
let quotaTrigger: HTMLElement | null = null;
const detailsForm = reactive({ nickname: "", tags: [] as string[], phone: "" });
const tagOptions = computed(() => editing.value ? [...new Set(
  platformRows(editing.value.target).flatMap(({ tags }) => tags),
)] : []);
const keyword = ref("");
const dialog = ref<"add" | "read" | null>(null);
const dialogSelection = ref("");
const showPlatformPicker = ref(false);
const dialogChoices = computed(() => dialog.value === "read" ? readChoices.value : choices.value);
const dialogPlatform = computed(() => dialogChoices.value.find(({ key }) => key === dialogSelection.value));
const method = ref<"login" | "json">("login");
const jsonInput = ref("");
const loginOption = ref("");
const login = ref<PlatformLogin | null>(null);
const code = ref("");
const error = ref("");
const loadErrors = ref<string[]>([]);
const starting = ref(false);
const completing = ref(false);
const submitting = ref(false);
let loadVersion = 0;
let loginVersion = 0;
let mounted = true;

function display(value: string | null): string {
  return value ? (props.displayText?.(value) ?? value) : "";
}

function platformRows(target: Choice): PlatformAccount[] {
  const entry = saved.value.get(target.key);
  return entry?.revision === target.revision ? entry.rows : [];
}

function accountKey(target: Choice, id: string): string {
  return JSON.stringify([target.key, id]);
}

const inlineQuotaLimit = 2;

function groupQuotas(quotas: PlatformQuota[]) {
  const metrics: PlatformQuota[] = [];
  const balances: PlatformQuota[] = [];
  for (const quota of quotas) {
    const balanceOnly = quota.balance !== null && quota.remainingPercent === null &&
      quota.resetAt === null && quota.used === null && quota.limit === null;
    (balanceOnly ? balances : metrics).push(quota);
  }
  return { metrics, balances };
}

const accounts = computed(() => {
  const targets = platform.value ? [platform.value] : choices.value;
  const rows = targets.flatMap((target) => platformRows(target).map((account) => ({
    target,
    account,
    key: accountKey(target, account.id),
    quotas: groupQuotas(account.quotas),
  })));
  return rows.sort((a, b) =>
    Number(!!b.account.pinned) - Number(!!a.account.pinned) ||
    (platform.value ? 0 : (b.account.updatedAt ?? 0) - (a.account.updatedAt ?? 0)),
  );
});
const filtered = computed(() => {
  const query = keyword.value.trim().toLowerCase();
  if (!query) return accounts.value;
  return accounts.value.filter(({ target, account }) =>
    target.label.toLowerCase().includes(query) ||
    [account.nickname, account.name, account.email, account.plan, ...account.tags]
      .some((part) => display(part ?? null).toLowerCase().includes(query)),
  );
});
const abnormalCount = computed(() => accounts.value.filter(({ account }) => account.quotaError).length);
const gridLayout = computed(() =>
  accountGridLayout(props.settings.maxColumns, props.viewportWidth, props.expandedLayout),
);
const hasBalance = computed(() => filtered.value.some(({ account, quotas }) =>
  account.quotas.length <= inlineQuotaLimit && quotas.balances.length > 0,
));
const quotaDetails = computed(() => quotaDetailsKey.value === null
  ? undefined
  : filtered.value.find(({ key }) => key === quotaDetailsKey.value),
);

watch(quotaDetails, (row) => {
  if (!row) quotaDetailsKey.value = null;
});

function openQuotaDetails(key: string, event: MouseEvent): void {
  quotaTrigger = event.currentTarget as HTMLElement;
  quotaDetailsKey.value = key;
}

function restoreQuotaFocus(): void {
  quotaTrigger?.focus({ preventScroll: true });
  quotaTrigger = null;
}

function isInstalled(target: Choice): boolean {
  return mounted && choices.value.some(({ key, revision }) => key === target.key && revision === target.revision);
}

function isCurrent(target: Choice, account: PlatformAccount): boolean {
  if (account.current !== undefined) return account.current;
  const entry = lastRead.value.get(target.key);
  return entry?.revision === target.revision && entry.ids.has(account.id);
}

function saveRows(target: Choice, rows: PlatformAccount[]): void {
  if (isInstalled(target)) saved.value.set(target.key, { revision: target.revision, rows });
}

function mergeRows(target: Choice, rows: PlatformAccount[]): void {
  const hasCurrent = rows.some((row) => row.current);
  const byId = new Map(platformRows(target).map((row) =>
    [row.id, hasCurrent && row.current ? { ...row, current: false } : row],
  ));
  for (const row of rows) byId.set(row.id, row);
  saveRows(target, [...byId.values()]);
}

function listRows(target: Choice): Promise<PlatformAccount[]> {
  return callPlatform(target.extensionId, target.id, { operation: "list" });
}

async function load(): Promise<void> {
  const targets = choices.value;
  const version = ++loadVersion;
  loading.value = true;
  loadErrors.value = [];
  const results = await Promise.allSettled(targets.map(listRows));
  if (version !== loadVersion) return;
  for (const [index, result] of results.entries()) {
    const target = targets[index];
    if (result.status === "fulfilled") saveRows(target, result.value);
    else loadErrors.value.push(`${target.label} · ${t(String(result.reason))}`);
  }
  loading.value = false;
}

function openRead(): void {
  if (busy.value || loading.value) return;
  if (platform.value) {
    void readCurrent(platform.value);
  } else {
    dialogSelection.value = "";
    showPlatformPicker.value = true;
    dialog.value = "read";
  }
}

async function readCurrent(target: Choice): Promise<void> {
  if (!isInstalled(target) || !target.localImport || busy.value || loading.value) return;
  if (dialog.value === "read") closeDialog();
  readingCurrent.value = true;
  try {
    const rows = await callPlatform<PlatformAccount[]>(target.extensionId, target.id, { operation: "import_current" });
    if (!isInstalled(target)) return;
    lastRead.value.set(target.key, { revision: target.revision, ids: new Set(rows.map(({ id }) => id)) });
    if (!rows.length) {
      Message.warning(t("未在本机找到登录账号，请先登录对应客户端。"));
      return;
    }
    keyword.value = "";
    mergeRows(target, rows);
    Message.success(t("当前账号已保存"));
  } catch (cause) {
    if (isInstalled(target)) Message.error(t(String(cause)));
  } finally {
    readingCurrent.value = false;
  }
}

async function updateAccount(
  target: Choice,
  id: string,
  action: string,
  apply: () => Promise<void>,
): Promise<boolean> {
  if (busy.value || loading.value || !isInstalled(target)) return false;
  updating.value = { key: accountKey(target, id), action };
  try {
    await apply();
    return true;
  } catch (cause) {
    if (isInstalled(target)) Message.error(t(String(cause)));
    return false;
  } finally {
    updating.value = null;
  }
}

function actionLoading(key: string, action: string): boolean {
  return updating.value?.key === key && updating.value.action === action;
}

function refresh(target: Choice, account: PlatformAccount): Promise<boolean> {
  return updateAccount(target, account.id, "refresh", async () => {
    const row = await callPlatform<PlatformAccount>(target.extensionId, target.id, {
      operation: "refresh",
      account_id: account.id,
    });
    if (isInstalled(target)) {
      mergeRows(target, [row]);
      if (row.quotaError) Message.warning(t("未能刷新额度"));
    }
  });
}

function togglePin(target: Choice, account: PlatformAccount): void {
  void updateAccount(target, account.id, "pin", async () => {
    const updated = await callPlatform<PlatformAccount>(target.extensionId, target.id, {
      operation: "set_pinned", account_id: account.id, pinned: !account.pinned,
    });
    mergeRows(target, [updated]);
  });
}

function openDetails(target: Choice, account: PlatformAccount, kind: "edit" | "phone"): void {
  detailsForm.nickname = account.nickname ?? "";
  detailsForm.tags = [...account.tags];
  detailsForm.phone = account.phone ?? "";
  editing.value = { target, account, kind };
}

function closeDetails(): void {
  if (!updating.value) editing.value = null;
}

async function saveDetails(): Promise<boolean> {
  const entry = editing.value;
  if (!entry || !isInstalled(entry.target)) return true;
  const { target, account, kind } = entry;
  const saved = await updateAccount(target, account.id, kind, async () => {
    const updated = await callPlatform<PlatformAccount>(target.extensionId, target.id, kind === "phone"
      ? { operation: "set_phone", account_id: account.id, phone: detailsForm.phone }
      : { operation: "edit", account_id: account.id, nickname: detailsForm.nickname, tags: detailsForm.tags });
    mergeRows(target, [updated]);
  });
  if (saved) closeDetails();
  return saved;
}

function switchAccount(target: Choice, account: PlatformAccount): void {
  Modal.confirm({
    title: t("切换账号"),
    content: `${target.label} · ${accountName(account)}。 ${t("切换会更新该客户端的本机登录账号，应用可能重启。")}`,
    okText: t("切换"),
    onBeforeOk: () => updateAccount(target, account.id, "switch", async () => {
      const result = await callPlatform<PlatformSwitchResult>(target.extensionId, target.id, {
        operation: "switch", account_id: account.id,
      });
      if (!isInstalled(target)) return;
      mergeRows(target, [result.account]);
      if (result.warning) Message.warning(t(result.warning));
      else Message.success(t("账号已切换"));
    }),
  });
}

function exportAccount(target: Choice, account: PlatformAccount): void {
  void updateAccount(target, account.id, "export", async () => {
    const json = await callPlatform<string>(target.extensionId, target.id, {
      operation: "export", account_id: account.id,
    });
    await props.downloadExport(json, `${target.id}-account.json`);
  });
}

function remove(target: Choice, account: PlatformAccount): void {
  Modal.confirm({
    title: t("删除账号"),
    content: `${target.label} · ${accountName(account)}`,
    okText: t("删除"),
    okButtonProps: { status: "danger" },
    onBeforeOk: () => !isInstalled(target) || updateAccount(target, account.id, "remove", async () => {
      await callPlatform(target.extensionId, target.id, { operation: "remove", account_id: account.id });
      saveRows(target, platformRows(target).filter(({ id }) => id !== account.id));
      lastRead.value.get(target.key)?.ids.delete(account.id);
    }),
  });
}

function cancelLogin(target: Choice, loginId: string): void {
  void callPlatform(target.extensionId, target.id, { operation: "login_cancel", login_id: loginId })
    .catch(() => undefined);
}

function clearLogin(target: Choice | null | undefined = dialogPlatform.value): void {
  loginVersion += 1;
  if (target && login.value) cancelLogin(target, login.value.loginId);
  login.value = null;
  code.value = "";
  error.value = "";
  starting.value = false;
  completing.value = false;
  submitting.value = false;
}

watch(() => JSON.stringify(choices.value.map(({ key, revision }) => [key, revision])), () => {
  if (!platform.value) selection.value = "";
  if (editing.value && !isInstalled(editing.value.target)) editing.value = null;
  void load();
}, { immediate: true });
watch(dialogPlatform, (target, previous) => {
  if (previous?.key === target?.key && previous?.revision === target?.revision) return;
  clearLogin(previous ?? null);
  loginOption.value = target?.loginOptions[0] ?? "";
  jsonInput.value = "";
});

onBeforeUnmount(() => {
  mounted = false;
  loadVersion += 1;
  clearLogin();
});

function openAdd(tab: "login" | "json" = "login"): void {
  if (busy.value || loading.value) return;
  method.value = tab;
  dialogSelection.value = platform.value?.key ?? "";
  showPlatformPicker.value = !platform.value;
  dialog.value = "add";
}

function changeAddMethod(key: string | number): void {
  clearLogin();
  method.value = key === "json" ? "json" : "login";
}

function closeDialog(): void {
  clearLogin();
  dialog.value = null;
  dialogSelection.value = "";
  jsonInput.value = "";
}

async function openLoginPage(current: PlatformLogin): Promise<void> {
  const url = new URL(current.url);
  if (url.protocol !== "https:" || url.username || url.password) {
    throw new Error(t("授权链接无效"));
  }
  await openExternalUrl(url.href);
}

async function startLogin(): Promise<void> {
  const target = dialogPlatform.value;
  if (!target || starting.value || login.value) return;
  const version = ++loginVersion;
  starting.value = true;
  error.value = "";
  try {
    const result = await callPlatform<PlatformLogin>(target.extensionId, target.id, {
      operation: "login_start",
      option: loginOption.value || undefined,
    });
    if (version !== loginVersion || dialog.value !== "add") {
      cancelLogin(target, result.loginId);
      return;
    }
    login.value = result;
    try {
      await openLoginPage(result);
    } catch {
      if (version === loginVersion) error.value = t("无法打开浏览器，请重试");
    }
    if (version === loginVersion && target.login !== "code") void finishLogin();
  } catch (cause) {
    if (version === loginVersion) error.value = String(cause);
  } finally {
    if (version === loginVersion) starting.value = false;
  }
}

async function reopenLogin(): Promise<void> {
  const current = login.value;
  if (!current) return;
  try {
    await openLoginPage(current);
    if (login.value === current) error.value = "";
  } catch {
    if (login.value === current) error.value = t("无法打开浏览器，请重试");
  }
}

async function finishLogin(): Promise<void> {
  const target = dialogPlatform.value;
  const current = login.value;
  if (!target || !current || completing.value) return;
  const version = loginVersion;
  completing.value = true;
  try {
    const result = await callPlatform<{ accountId: string }>(target.extensionId, target.id, {
      operation: "login_finish",
      login_id: current.loginId,
      code: code.value.trim() || undefined,
    });
    if (!isInstalled(target)) return;
    if (version === loginVersion) {
      login.value = null;
      closeDialog();
      Message.success(t("账号已添加"));
    }
    const updated = await updateAccount(target, result.accountId, "login", async () => {
      saveRows(target, await listRows(target));
    });
    const account = platformRows(target).find(({ id }) => id === result.accountId);
    if (updated && account) void refresh(target, account);
  } catch (cause) {
    if (version === loginVersion) {
      error.value = String(cause);
      if (target.login !== "code") {
        cancelLogin(target, current.loginId);
        login.value = null;
      }
    }
  } finally {
    if (version === loginVersion) completing.value = false;
  }
}

async function submitCallback(): Promise<void> {
  const target = dialogPlatform.value;
  const current = login.value;
  if (!target || !current || !code.value.trim() || submitting.value) return;
  const version = loginVersion;
  submitting.value = true;
  error.value = "";
  try {
    await callPlatform(target.extensionId, target.id, {
      operation: "login_callback",
      login_id: current.loginId,
      code: code.value.trim(),
    });
    if (version === loginVersion) {
      code.value = "";
      void finishLogin();
    }
  } catch (cause) {
    if (version === loginVersion) error.value = String(cause);
  } finally {
    if (version === loginVersion) submitting.value = false;
  }
}

async function copyDeviceCode(value: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(value);
    Message.success(t("已复制"));
  } catch {
    Message.error(t("复制失败"));
  }
}

async function importJson(): Promise<void> {
  const target = dialogPlatform.value;
  if (!target || !jsonInput.value.trim() || submitting.value) return;
  const version = loginVersion;
  submitting.value = true;
  error.value = "";
  try {
    const rows = await callPlatform<PlatformAccount[]>(target.extensionId, target.id, { operation: "import", json: jsonInput.value });
    mergeRows(target, rows);
    if (version === loginVersion) {
      closeDialog();
      Message.success(t("账号已导入"));
    }
  } catch (cause) {
    if (version === loginVersion) error.value = String(cause);
  } finally {
    if (version === loginVersion) submitting.value = false;
  }
}

function accountName(account: PlatformAccount): string {
  return display(account.nickname || account.name || account.email) || t("账号");
}

function accountId(id: string): string {
  const value = props.displayText ? display(id) : id;
  return value.length <= 8 ? value : `${value.slice(0, 3)}****${value.slice(-3)}`;
}

function badgeClass(plan: string): string {
  const normalized = plan.toLowerCase().replace(/[_-]/g, " ");
  const key = /\b(free|trial|basic)\b/.test(normalized) ? "free"
    : /\bplus\b/.test(normalized) ? "plus"
      : /\b(pro|max|super|ultra)\b/.test(normalized) ? "proMax" : "team";
  const tone = key === "proMax" ? "pro-max" : key;
  const style = props.settings.badgeStyles?.[key] || props.settings.badgeStyle || "classic";
  return `${tone} badge-${style}`;
}

</script>

<template>
  <template v-if="choices.length">
    <header class="topbar platform-topbar" :aria-label="t('其他平台')">
      <div class="brand">
        <div class="status-line">
          <a-tag color="arcoblue">{{ t("全部") }} {{ accounts.length }}</a-tag>
          <a-tag v-if="abnormalCount" color="red">{{ t("异常账号") }} {{ abnormalCount }}</a-tag>
        </div>
      </div>
      <div class="command-actions">
        <a-button class="command-button command-quota" :loading="loading" :disabled="busy" @click="load()">
          <template #icon><icon-refresh /></template>
          {{ t("刷新列表") }}
        </a-button>
        <a-button
          class="command-button command-privacy"
          :class="{ 'is-active': displayText }"
          :aria-pressed="!!displayText"
          @click="emit('toggle-privacy')"
        >
          <template #icon>
            <icon-eye-invisible v-if="displayText" />
            <icon-eye v-else />
          </template>
          {{ displayText ? t("已隐藏") : t("隐私") }}
        </a-button>
        <a-button class="command-button command-badge" @click="emit('open-badge-style')">
          <template #icon><icon-palette /></template>
          {{ t("徽章样式") }}
        </a-button>
        <a-button
          v-if="platform ? platform.localImport : readChoices.length"
          class="command-button command-detect"
          :loading="readingCurrent"
          :disabled="loading || busy"
          @click="openRead"
        >
          <template #icon><icon-refresh /></template>
          {{ t("读取当前账号") }}
        </a-button>
        <a-button type="primary" class="command-button command-add" :disabled="loading || busy" @click="openAdd()">
          <template #icon><icon-plus /></template>
          {{ t("添加账号") }}
        </a-button>
      </div>
    </header>

    <section class="account-ops platform-account-ops" :aria-label="t('平台')">
      <div class="account-ops-left">
        <PlatformPicker v-model="selection" :choices="choices" allow-all class="sort-select" />
        <a-input
          v-model="keyword"
          class="account-search-input"
          :placeholder="t('筛选邮箱 / 昵称 / 标签')"
          :input-attrs="{ 'aria-label': t('筛选邮箱 / 昵称 / 标签') }"
          allow-clear
        >
          <template #prefix><icon-search /></template>
        </a-input>
        <div class="account-batch-actions">
          <a-button class="batch-action batch-import" :disabled="loading || busy" @click="openAdd('json')">
            <template #icon><icon-import /></template>
            {{ t("导入 JSON") }}
          </a-button>
        </div>
      </div>
    </section>

    <section class="accounts-page-content">
      <a-alert v-for="(message, index) in loadErrors" :key="index" type="error">{{ message }}</a-alert>
      <a-spin :loading="loading" class="accounts-spin" dot>
        <section
          v-if="filtered.length"
          class="account-grid"
          :class="[gridLayout.classes, { 'platform-grid-with-balance': hasBalance }]"
          :style="gridLayout.style"
        >
          <a-card
            v-for="{ target, account, key, quotas } in filtered"
            :key="key"
            class="account-card platform-account-card"
            :class="{ active: isCurrent(target, account), pinned: account.pinned, 'platform-card-legacy': !target.accountActions }"
            :bordered="false"
          >
            <div class="account-head">
              <div class="account-identity-card">
                <span class="account-check-zone platform-account-icon" aria-hidden="true">
                  <img v-if="target.iconUrl" :src="target.iconUrl" alt="" />
                  <icon-apps v-else />
                </span>
                <div class="account-title">
                  <div class="account-title-main">
                    <span
                      class="account-name"
                      :title="account.email && (account.nickname || account.name) !== account.email
                        ? `${accountName(account)} · ${display(account.email)}` : accountName(account)"
                    >{{ accountName(account) }}</span>
                  </div>
                  <div class="account-action-meta">
                    <small :title="`${t('账号 ID')} · ${accountId(account.id)}`">{{ t("账号 ID") }}: {{ accountId(account.id) }}</small>
                    <a-tooltip v-if="isCurrent(target, account)" :content="t('最近读取或切换的账号')">
                      <span class="current-account-pill identity-current-pill">{{ t("当前") }}</span>
                    </a-tooltip>
                    <a-tooltip v-if="account.tags.length" :content="account.tags.map(display).join(' / ')">
                      <span class="account-tag-list">
                        <span v-for="(tag, index) in account.tags.slice(0, 2)" :key="index" class="account-tag-pill">{{ display(tag) }}</span>
                        <span v-if="account.tags.length > 2" class="account-tag-pill more">+{{ account.tags.length - 2 }}</span>
                      </span>
                    </a-tooltip>
                    <PlanBadge v-if="account.plan" :label="account.plan.toUpperCase()" :badge-class="badgeClass(account.plan)" />
                  </div>
                  <span class="platform-account-label">{{ target.label }}</span>
                </div>
              </div>
            </div>

            <div class="account-health">
              <button
                v-if="account.quotas.length > inlineQuotaLimit"
                class="platform-quota-trigger"
                type="button"
                aria-haspopup="dialog"
                :aria-expanded="quotaDetailsKey === key"
                @click="openQuotaDetails(key, $event)"
              >
                <icon-bar-chart />
                <span>{{ t("额度详情") }}</span>
                <small>{{ formatLocalizedCount(account.quotas.length, "项") }}</small>
                <icon-right />
              </button>
              <PlatformQuotaList
                v-else-if="account.quotas.length"
                v-bind="quotas"
                :status-clock-ms="statusClockMs"
                :hide-amounts="!!displayText"
              />
              <div v-if="account.quotaError" class="quota-error platform-quota-status">{{ t("未能刷新额度") }}</div>
              <p v-else-if="!account.quotas.length" class="platform-quota-status">{{ t("暂无额度数据") }}</p>
            </div>

            <footer class="card-footer">
              <div class="footer-meta">
                <small class="account-last-used" :title="t('更新时间')">
                  {{ account.updatedAt === null ? "—" : formatWallTime(account.updatedAt) }}
                </small>
                <button
                  v-if="target.accountActions && account.phone"
                  class="footer-phone"
                  type="button"
                  :disabled="loading || busy"
                  :aria-label="t('绑定手机')"
                  @click="openDetails(target, account, 'phone')"
                >
                  <icon-phone />
                  <span>{{ display(account.phone) }}</span>
                </button>
              </div>
              <div class="card-actions">
                <template v-if="target.accountActions">
                  <a-tooltip :content="t(account.pinned ? '取消置顶' : '置顶账号')">
                    <a-button
                      size="small"
                      :aria-label="t(account.pinned ? '取消置顶' : '置顶账号')"
                      :class="{ 'action-active': account.pinned }"
                      :loading="actionLoading(key, 'pin')"
                      :disabled="loading || busy"
                      @click="togglePin(target, account)"
                    >
                      <template #icon><icon-pushpin /></template>
                    </a-button>
                  </a-tooltip>
                  <a-tooltip :content="t('绑定手机')">
                    <a-button
                      size="small"
                      :aria-label="t('绑定手机')"
                      :disabled="loading || busy"
                      @click="openDetails(target, account, 'phone')"
                    >
                      <template #icon><icon-phone /></template>
                    </a-button>
                  </a-tooltip>
                  <a-tooltip :content="t('编辑')">
                    <a-button
                      size="small"
                      :aria-label="t('编辑')"
                      :disabled="loading || busy"
                      @click="openDetails(target, account, 'edit')"
                    >
                      <template #icon><icon-edit /></template>
                    </a-button>
                  </a-tooltip>
                  <a-tooltip :content="t('切换账号')">
                    <a-button
                      size="small"
                      :aria-label="t('切换账号')"
                      :loading="actionLoading(key, 'switch')"
                      :disabled="loading || busy"
                      @click="switchAccount(target, account)"
                    >
                      <template #icon><icon-play-arrow /></template>
                    </a-button>
                  </a-tooltip>
                </template>
                <a-tooltip :content="t('刷新额度')">
                  <a-button
                    size="small"
                    :aria-label="t('刷新额度')"
                    :loading="actionLoading(key, 'refresh')"
                    :disabled="loading || busy"
                    @click="refresh(target, account)"
                  >
                    <template #icon><icon-refresh /></template>
                  </a-button>
                </a-tooltip>
                <a-tooltip v-if="target.accountActions" :content="t('导出')">
                  <a-button
                    size="small"
                    :aria-label="t('导出')"
                    :loading="actionLoading(key, 'export')"
                    :disabled="loading || busy"
                    @click="exportAccount(target, account)"
                  >
                    <template #icon><icon-download /></template>
                  </a-button>
                </a-tooltip>
                <a-tooltip :content="t('删除账号')">
                  <a-button
                    size="small"
                    :aria-label="t('删除账号')"
                    :loading="actionLoading(key, 'remove')"
                    :disabled="loading || busy"
                    @click="remove(target, account)"
                  >
                    <template #icon><icon-delete /></template>
                  </a-button>
                </a-tooltip>
              </div>
            </footer>
          </a-card>
        </section>

        <section v-else-if="!loading && keyword.trim()" class="account-filter-empty">
          <a-empty :description="t('没有匹配的账号')" />
        </section>

        <section v-else-if="!loading && !loadErrors.length" class="empty-wrap">
          <div class="empty-panel">
            <div class="empty-copy">
              <span class="empty-kicker">{{ platform?.label ?? t("全部平台") }}</span>
              <h2>{{ t("暂无账号") }}</h2>
              <p>{{ t("可在浏览器授权，或导入该平台的账号 JSON。") }}</p>
              <div class="empty-actions">
                <a-button type="primary" size="large" :disabled="busy" @click="openAdd()">
                  <template #icon><icon-plus /></template>
                  {{ t("添加账号") }}
                </a-button>
                <a-button size="large" :disabled="busy" @click="openAdd('json')">
                  <template #icon><icon-import /></template>
                  {{ t("导入 JSON") }}
                </a-button>
              </div>
            </div>
            <div class="empty-preview" aria-hidden="true">
              <div class="preview-card preview-card-main">
                <div class="preview-head">
                  <span class="preview-dot" />
                  <span class="preview-line short" />
                </div>
                <div class="preview-line wide" />
                <div class="preview-line" />
                <div class="preview-footer"><span /><span /></div>
              </div>
              <div class="preview-card preview-card-back">
                <div class="preview-head">
                  <span class="preview-dot muted" />
                  <span class="preview-line short" />
                </div>
                <div class="preview-line wide" />
                <div class="preview-line short" />
              </div>
            </div>
          </div>
        </section>
      </a-spin>
    </section>

    <a-modal
      :visible="!!quotaDetails"
      :title="t('额度详情')"
      :ok-text="t('关闭')"
      :hide-cancel="true"
      width="min(600px, 94vw)"
      :body-style="{ padding: '16px clamp(16px, 4vw, 28px) 22px' }"
      role="dialog"
      aria-modal="true"
      :aria-label="t('额度详情')"
      unmount-on-close
      @update:visible="!$event && (quotaDetailsKey = null)"
      @open="quotaScroll?.focus()"
      @close="restoreQuotaFocus"
    >
      <template v-if="quotaDetails">
        <p class="platform-quota-context">{{ quotaDetails.target.label }} · {{ accountName(quotaDetails.account) }}</p>
        <div ref="quotaScroll" class="platform-quota-scroll" tabindex="0">
          <PlatformQuotaList
            v-bind="quotaDetails.quotas"
            :status-clock-ms="statusClockMs"
            :hide-amounts="!!displayText"
          />
          <div v-if="quotaDetails.account.quotaError" class="quota-error platform-quota-status">
            {{ t("未能刷新额度") }}
          </div>
        </div>
      </template>
    </a-modal>

    <a-modal
      :visible="editing?.kind === 'edit'"
      :title="t('编辑账号')"
      :ok-text="t('保存')"
      :mask-closable="false"
      :on-before-ok="saveDetails"
      @cancel="closeDetails"
    >
      <a-form :model="detailsForm" layout="vertical">
        <a-form-item :label="t('账号名称')">
          <a-input
            v-model="detailsForm.nickname"
            :placeholder="editing ? accountName(editing.account) : ''"
            :max-length="120"
            :input-attrs="{ 'aria-label': t('账号名称') }"
          />
        </a-form-item>
        <a-form-item :label="t('标签')">
          <a-select
            v-model="detailsForm.tags"
            :placeholder="t('选择或输入标签')"
            :aria-label="t('标签')"
            multiple
            allow-create
            allow-clear
          >
            <a-option v-for="tag in tagOptions" :key="tag" :value="tag">{{ tag }}</a-option>
          </a-select>
        </a-form-item>
      </a-form>
    </a-modal>

    <PhoneModal
      :visible="editing?.kind === 'phone'"
      :account-name="editing ? accountName(editing.account) : ''"
      :phone-form="detailsForm"
      :max-length="60"
      :saving="busy"
      @update:visible="!$event && closeDetails()"
      @save="saveDetails"
    />

    <a-modal
      :visible="dialog !== null"
      :title="t(dialog === 'read' ? '读取当前账号' : method === 'json' ? '导入 JSON' : '添加账号')
        + (dialogPlatform && !showPlatformPicker ? ` · ${dialogPlatform.label}` : '')"
      :footer="false"
      :mask-closable="false"
      @cancel="closeDialog"
    >
      <PlatformPicker
        v-if="showPlatformPicker"
        v-model="dialogSelection"
        :choices="dialogChoices"
        :disabled="starting || submitting || !!login"
        class="platform-dialog-picker"
      />
      <a-button
        v-if="dialog === 'read'"
        type="primary"
        :disabled="!dialogPlatform"
        @click="dialogPlatform && readCurrent(dialogPlatform)"
      >{{ t("读取当前账号") }}</a-button>
      <a-tabs v-else-if="dialogPlatform" :active-key="method" @change="changeAddMethod">
        <a-tab-pane key="login" :title="t('浏览器授权')">
          <div class="platform-dialog">
            <a-select v-if="dialogPlatform.loginOptions.length && !login" v-model="loginOption" :aria-label="t('授权方式')">
              <a-option v-for="option in dialogPlatform.loginOptions" :key="option" :value="option">
                {{ ({ en: 'Global', cn: '中国', zai: 'Z.AI', bigmodel: 'BigModel' })[option] ?? option }}
              </a-option>
            </a-select>
            <a-button v-if="!login" type="primary" :loading="starting" @click="startLogin">{{ t("打开浏览器授权") }}</a-button>
            <template v-else>
              <div v-if="login.userCode" class="platform-device">
                <span>{{ t("设备验证码") }}</span><code>{{ login.userCode }}</code>
                <a-button size="small" @click="copyDeviceCode(login.userCode)">{{ t("复制") }}</a-button>
              </div>
              <span v-if="completing && dialogPlatform.login !== 'code'">{{ t("正在等待浏览器授权") }}</span>
              <a-button type="outline" @click="reopenLogin">{{ t("重新打开授权页") }}</a-button>
              <a-input
                v-if="dialogPlatform.login !== 'browser'"
                v-model="code"
                :placeholder="t(dialogPlatform.login === 'code' ? '粘贴授权码' : '也可以粘贴授权回调地址')"
                :input-attrs="{ autocomplete: 'off', spellcheck: false }"
              />
              <a-button
                v-if="dialogPlatform.login !== 'browser'"
                type="primary"
                :disabled="!code.trim()"
                :loading="dialogPlatform.login === 'code' ? completing : submitting"
                @click="dialogPlatform.login === 'code' ? finishLogin() : submitCallback()"
              >{{ t("提交授权") }}</a-button>
            </template>
          </div>
        </a-tab-pane>
        <a-tab-pane key="json" :title="t('导入 JSON')">
          <div class="platform-dialog">
            <a-textarea v-model="jsonInput" :rows="7" :max-length="500000" :placeholder="t('粘贴该平台导出的账号 JSON')" />
            <a-button type="primary" :loading="submitting" :disabled="!jsonInput.trim()" @click="importJson">{{ t("导入") }}</a-button>
          </div>
        </a-tab-pane>
      </a-tabs>
      <a-alert v-if="error" type="error" class="platform-error">{{ t(error) }}</a-alert>
    </a-modal>
  </template>
</template>

<style scoped>
.platform-topbar,
.platform-topbar .command-actions {
  flex-wrap: wrap;
}
.platform-account-ops .account-batch-actions {
  margin-left: auto;
}
.platform-grid-with-balance {
  --additional-quota-height: 46px;
}
.account-grid .account-card.platform-account-card {
  height: auto;
}
.account-grid .account-card.platform-account-card :deep(.arco-card-body) {
  min-height: 100%;
  border: 0;
}
.account-grid .platform-account-card .account-health {
  padding-bottom: 14px;
}
.platform-account-icon {
  border-radius: 8px;
  color: #4e6280;
  background: #eff4fb;
  font-size: 17px;
}
.platform-account-icon img {
  width: 21px;
  height: 21px;
  object-fit: contain;
}
.platform-account-label {
  color: var(--text-soft);
  font-size: 11px;
  font-weight: 600;
}
.platform-account-card .account-action-meta small {
  flex: 1 1 0;
}
.platform-account-card .account-action-meta :deep(.plan-badge) {
  flex-shrink: 1;
  min-width: 0;
  max-width: 46%;
}
.platform-account-card .account-tag-list {
  flex: 0 1 auto;
  min-width: 0;
  max-width: 45%;
  overflow: hidden;
}
.platform-account-card :deep(.plan-badge-label),
.platform-account-card .account-tag-pill {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
.platform-account-card .account-tag-pill {
  display: block;
  max-width: 8em;
  line-height: 20px;
}
.platform-account-card .account-tag-pill:only-child {
  max-width: 100%;
}
.empty-preview .preview-head .preview-line {
  height: 11px;
  margin-top: 0;
}
.platform-quota-trigger {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 44px;
  padding: 8px 12px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: #f5f8fc;
  color: var(--blue);
  font-size: 13px;
  font-weight: 720;
  text-align: start;
  cursor: pointer;
}
.platform-quota-trigger:hover {
  border-color: var(--line-strong);
  background: #edf4ff;
}
.platform-quota-trigger small {
  flex: 0 0 auto;
  margin-inline-start: auto;
  color: var(--text-soft);
  font-size: 11px;
  font-weight: 650;
}
.platform-quota-trigger > svg {
  flex: 0 0 auto;
}
.platform-quota-context {
  margin: 0 0 12px;
  color: var(--text-soft);
  overflow-wrap: anywhere;
}
.platform-quota-scroll {
  max-height: min(54vh, 520px);
  overflow-y: auto;
  overscroll-behavior: contain;
}
.platform-quota-scroll:deep(.quota-metrics) {
  grid-auto-rows: minmax(70px, auto);
}
.platform-quota-scroll:deep(.quota-window-label > span) {
  white-space: normal;
  overflow-wrap: anywhere;
}
.platform-quota-status {
  margin: 8px 0 0;
  color: var(--text-soft);
  font-size: 12px;
}
.platform-quota-status.quota-error {
  color: #b42318;
}
.account-grid .account-card.platform-card-legacy .card-actions {
  justify-content: flex-start;
  gap: 8px;
}
.platform-dialog { display: grid; gap: 10px; }
.platform-dialog-picker { margin-bottom: 12px; }
.platform-device { display: flex; gap: 10px; align-items: center; }
.platform-error { margin-top: 12px; }
</style>
