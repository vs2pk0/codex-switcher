import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import test from "node:test";
import ts from "typescript";
import type { CodexAccount } from "../src/types/codex.ts";

const account: CodexAccount = {
  id: "oauth", email: "fixture@example.com", created_at: 0, last_used: 0,
  subscription_active_until: "2099-01-01T00:00:00Z",
  tokens: { id_token: "", access_token: "valid" },
};
const source = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");
const handler = source.slice(source.indexOf("async function handleRefreshQuota("), source.indexOf("async function refreshAccountsAfterMutation("));
const compiled = ts.transpileModule(handler, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;

function fixture(updated: CodexAccount) {
  const calls: unknown[] = [];
  const success: string[] = [];
  const warnings: string[] = [];
  const context = {
    quotaRefreshingId: { value: "" },
    accounts: { value: [account] },
    async refreshCodexQuota(id: string, forceSubscriptionRefresh: boolean) {
      calls.push([id, forceSubscriptionRefresh]);
      return updated;
    },
    async loadAccounts() { context.accounts.value = [updated]; },
    errorText: String,
    formatTranslatedText: (text: string, values: { error: string }) => text.replace("{error}", values.error),
    Message: { success: (text: string) => success.push(text), warning: (text: string) => warnings.push(text) },
  };
  const run = runInNewContext(`${compiled}\nhandleRefreshQuota`, context) as (account: CodexAccount) => Promise<void>;
  return { context, run, calls, success, warnings };
}

test("手动刷新额度强制同步订阅，并加载续费后的到期时间", async () => {
  const updated = { ...account, subscription_active_until: "2099-02-01T00:00:00Z" };
  const state = fixture(updated);
  await state.run(account);
  assert.deepEqual(state.calls, [[account.id, true]]);
  assert.equal(state.context.accounts.value[0].subscription_active_until, updated.subscription_active_until);
  assert.deepEqual(state.success, ["额度已刷新"]);
  assert.deepEqual(state.warnings, []);
  assert.equal(state.context.quotaRefreshingId.value, "");
});

test("额度成功但订阅失败时保留结果并明确提示，不误报全部刷新成功", async () => {
  const updated = { ...account, subscription_query_last_error: "订阅信息接口返回 401" };
  const state = fixture(updated);
  await state.run(account);
  assert.equal(state.context.accounts.value[0], updated);
  assert.deepEqual(state.success, []);
  assert.deepEqual(state.warnings, ["额度已刷新，订阅状态未更新：订阅信息接口返回 401"]);
  assert.equal(state.context.quotaRefreshingId.value, "");
});

test("额度请求失败仍恢复按钮状态并提示实际失败", async () => {
  const state = fixture(account);
  state.context.refreshCodexQuota = async () => { throw "network timeout"; };
  await state.run(account);
  assert.deepEqual(state.success, []);
  assert.deepEqual(state.warnings, ["额度刷新失败：network timeout"]);
  assert.equal(state.context.quotaRefreshingId.value, "");
});
