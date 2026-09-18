import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { stackQuotaAccounts } from "../src/quota.ts";

const modalSource = readFileSync(new URL("../src/components/EditAccountModal.vue", import.meta.url), "utf8");
const serviceSource = readFileSync(new URL("../src/services/quotaList.ts", import.meta.url), "utf8");
const backendSource = readFileSync(new URL("../src-tauri/src/quota_list.rs", import.meta.url), "utf8");
const libSource = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

test("叠加显示按窗口时长分桶，任一账号有的窗口都会显示", () => {
  const a1 = {
    accountId: "a1",
    windows: [
      { key: "hourly", label: "5 小时", percentage: 100, windowMinutes: 300, resetTime: 2000 },
      { key: "weekly", label: "7 天", percentage: 50, windowMinutes: 10080, resetTime: 9000 },
      {
        key: "2:gpt 5.3 codex spark:primary:18000",
        label: "gpt 5.3 codex spark",
        percentage: 20,
        windowMinutes: 300,
      },
    ],
  };
  const a2 = {
    accountId: "a2",
    windows: [
      { key: "hourly", label: "5 小时", percentage: 80, windowMinutes: 300, resetTime: 1000 },
      { key: "weekly", label: "7 天", percentage: 60, windowMinutes: 10080, resetTime: 8000 },
      {
        key: "0:gpt 5.3 codex spark:primary:18000",
        label: "gpt 5.3 codex spark",
        percentage: 30,
        windowMinutes: 300,
      },
    ],
  };
  // Free：只有 30 天月额度，不应把 Plus 的 5h / 周额度盖掉
  const a3 = {
    accountId: "a3",
    windows: [{ key: "hourly", label: "30 天", percentage: 40, windowMinutes: 43200 }],
  };

  const buckets = stackQuotaAccounts([a1, a2]);
  assert.deepEqual(
    buckets.map((bucket) => bucket.key),
    ["hourly:300", "weekly:10080", "named:gpt 5.3 codex spark:300"],
  );
  assert.equal(buckets[0].percentage, 180);
  assert.equal(buckets[0].accountCount, 2);
  assert.equal(buckets[0].resetTime, 1000);
  assert.equal(buckets[0].windowMinutes, 300);
  assert.equal(buckets[1].percentage, 110);
  assert.equal(buckets[1].windowMinutes, 10080);
  assert.equal(buckets[1].resetTime, 8000);
  assert.equal(buckets[2].percentage, 50);
  assert.equal(buckets[2].resetTime, undefined);

  const mixed = stackQuotaAccounts([a1, a2, a3, { accountId: "a4", windows: [] }]);
  assert.deepEqual(
    mixed.map((bucket) => bucket.key),
    ["hourly:300", "hourly:43200", "weekly:10080", "named:gpt 5.3 codex spark:300"],
  );
  assert.equal(mixed[0].percentage, 180);
  assert.equal(mixed[0].accountCount, 2);
  assert.equal(mixed[1].percentage, 40);
  assert.equal(mixed[1].accountCount, 1);
  assert.equal(mixed[1].windowMinutes, 43200);
  assert.equal(mixed[2].percentage, 110);
  assert.equal(mixed[2].accountCount, 2);
});

test("额度列表命令带 2 分钟缓存并注册到 Tauri", () => {
  assert.match(backendSource, /QUOTA_LIST_CACHE_SECONDS: i64 = 120/);
  assert.match(backendSource, /pub\(crate\) async fn check_quota_list_status/);
  assert.match(backendSource, /pub\(crate\) async fn fetch_quota_list/);
  assert.match(backendSource, /checkListStatus/);
  assert.match(backendSource, /quotaList/);
  assert.match(backendSource, /fn quota_window_present/);
  assert.match(libSource, /quota_list::check_quota_list_status,/);
  assert.match(libSource, /quota_list::fetch_quota_list,/);
  assert.match(serviceSource, /invoke<QuotaListStatus>\("check_quota_list_status"/);
  assert.match(serviceSource, /invoke<QuotaListResult>\("fetch_quota_list", \{ baseUrl, force \}\)/);
});

test("列表额度开关按账号持久化", () => {
  const accountBackendSource = readFileSync(
    new URL("../src-tauri/src/account.rs", import.meta.url),
    "utf8",
  );
  const codexServiceSource = readFileSync(new URL("../src/services/codex.ts", import.meta.url), "utf8");
  const appSource = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");
  assert.match(accountBackendSource, /pub quota_list_enabled: bool/);
  assert.match(accountBackendSource, /pub quota_list_stacked: bool/);
  assert.match(accountBackendSource, /pub quota_list_enabled: Option<bool>/);
  assert.match(accountBackendSource, /pub quota_list_stacked: Option<bool>/);
  assert.match(accountBackendSource, /account\.quota_list_enabled = quota_list_enabled/);
  assert.match(codexServiceSource, /quotaListEnabled: input\.quotaListEnabled \?\? null/);
  assert.match(codexServiceSource, /quotaListStacked: input\.quotaListStacked \?\? null/);
  assert.match(appSource, /editForm\.quotaListEnabled = Boolean\(account\.quota_list_enabled\)/);
  assert.match(appSource, /quotaListEnabled: editForm\.quotaListEnabled/);
  assert.match(appSource, /quotaListStacked: editForm\.quotaListStacked/);
});

test("编辑弹窗只保留开关，额度列表在账号卡片按额度面板样式显示", () => {
  const accountListSource = readFileSync(
    new URL("../src/components/AccountList.vue", import.meta.url),
    "utf8",
  );
  const stylesSource = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
  const appSource = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");
  assert.match(modalSource, /checkQuotaListStatus\(baseUrl\)/);
  assert.match(modalSource, /v-if="quotaListAvailable"/);
  assert.match(modalSource, /v-model="editForm\.quotaListEnabled"/);
  assert.match(modalSource, /editForm\.quotaListStacked = true/);
  assert.match(modalSource, /editForm\.quotaListStacked = false/);
  assert.doesNotMatch(modalSource, /quota-list-box/);
  assert.match(accountListSource, /quotaListStates: Record<string, QuotaListState>/);
  assert.match(accountListSource, /stackQuotaAccounts\(quotaListAccounts\(account\)\)/);
  // 附加窗口（GPT 5.3 Codex Spark 等）跟随「显示 GPT 5.3 Codex Spark 额度」设置过滤。
  assert.match(accountListSource, /showAdditional \? item : \{ \.\.\.item, windows: item\.windows\.filter\(isPrimaryQuotaListWindow\) \}/);
  // API 账号卡片保持原高度；账号切换列表固定两行并隐藏滚动条。
  assert.match(accountListSource, /if \(quotaListEnabledAccount\(account\)\) return 2;/);
  assert.match(stylesSource, /\.api-quota-tabs\s*\{[\s\S]*?max-height: 70px;[\s\S]*?overflow-y: auto;[\s\S]*?scrollbar-width: none;/);
  assert.match(stylesSource, /\.api-quota-tabs::-webkit-scrollbar\s*\{[\s\S]*?display: none;/);
  assert.match(accountListSource, /class="quota-panel api-quota-panel"/);
  assert.match(accountListSource, /v-for="metric in quotaListMetrics\(account\)"/);
  assert.match(accountListSource, /quotaListMetricCaption\(account, metric\)/);
  assert.match(accountListSource, /stacked: quotaListStackedMode\(account\)/);
  assert.match(
    accountListSource,
    /<div class="quota-bar">\s*<span :style="quotaProgressStyle\(metric\.percentage\)" \/>/,
  );
  assert.match(accountListSource, /v-if="!quotaListEnabledAccount\(account\)" class="api-key-info-row"/);
  assert.match(
    accountListSource,
    /v-if="apiOfficialUrl\(account\) && !quotaListEnabledAccount\(account\)"/,
  );
  assert.match(accountListSource, /quotaListTabAccounts\(account\)\.length > 1/);
  assert.match(accountListSource, /api-quota-tab/);
  assert.match(accountListSource, /emit\('refresh-quota-list', account\)/);
  assert.doesNotMatch(accountListSource, /api-quota-info-row/);
  assert.ok(
    accountListSource.indexOf('class="quota-metrics"') <
      accountListSource.indexOf('class="api-quota-tabs"'),
    "账号切换应显示在额度行下方",
  );
  assert.match(appSource, /const quotaListStates = reactive<Record<string, QuotaListState>>/);
  assert.match(appSource, /fetchQuotaList\(baseUrl, force\)/);
  assert.match(appSource, /@refresh-quota-list="handleRefreshQuotaList"/);
});
