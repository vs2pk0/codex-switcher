import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import test from "node:test";
import ts from "typescript";
import { canRefreshAccountToken } from "../src/accountStatus.ts";
import { beginPendingItem, finishPendingItem } from "../src/services/resetUiState.ts";
import type { CodexAccount } from "../src/types/codex.ts";

const account: CodexAccount = {
  id: "oauth", email: "fixture@example.com", created_at: 0, last_used: 0,
  access_token_expires_at: "2099-01-01T00:00:00Z",
  tokens: { id_token: "", access_token: "valid", refresh_token: "renewal" },
};
const source = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");
const handler = source.slice(source.indexOf("async function handleRefreshToken("), source.indexOf("async function handleRefreshQuota("));
const compiled = ts.transpileModule(handler, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;

function fixture() {
  const calls: string[] = [];
  const success: string[] = [];
  const errors: string[] = [];
  const context = {
    accounts: { value: [account] },
    currentAccount: { value: account },
    tokenRefreshingIds: { value: [] as string[] },
    // Manual renewal must remain usable with both background settings off.
    settings: { tokenKeeperEnabled: false, monitorQuota: false },
    canRefreshAccountToken, beginPendingItem, finishPendingItem,
    t: (text: string) => text,
    errorText: String,
    formatTranslatedText: (text: string, values: { error: string }) => text.replace("{error}", values.error),
    Message: { success: (text: string) => success.push(text), error: (text: string) => errors.push(text) },
    async refreshCodexAccountToken(id: string): Promise<CodexAccount> {
      calls.push(id);
      return { ...account, access_token_expires_at: "2099-02-01T00:00:00Z" };
    },
    async loadAccounts() { calls.push("reload"); },
  };
  const run = runInNewContext(`${compiled}\nhandleRefreshToken`, context) as (account: CodexAccount) => Promise<void>;
  return { context, run, calls, success, errors };
}

test("只有具备续期凭据的 OAuth 账号可以手动续期，不受当前 Token 到期时间限制", () => {
  assert.equal(canRefreshAccountToken(account), true);
  assert.equal(canRefreshAccountToken({ ...account, access_token_expires_at: "2000-01-01" }), true);
  assert.equal(canRefreshAccountToken({ ...account, access_token_expires_at: undefined }), true);
  for (const refresh_token of [undefined, "", " \n "]) {
    assert.equal(canRefreshAccountToken({ ...account, tokens: { ...account.tokens, refresh_token } }), false);
  }
  for (const api of [{ auth_mode: "apikey" }, { openai_api_key: "key" }, { openaiApiKey: "key" }]) {
    assert.equal(canRefreshAccountToken({ ...account, ...api }), false);
  }
});

test("关闭自动续期后仍立即续期，并更新列表和当前账号的到期时间", async () => {
  const state = fixture();
  await state.run(account);
  assert.deepEqual(state.calls, [account.id, "reload"]);
  assert.equal(state.context.accounts.value[0].access_token_expires_at, "2099-02-01T00:00:00Z");
  assert.equal(state.context.currentAccount.value, state.context.accounts.value[0]);
  assert.equal(state.context.tokenRefreshingIds.value.length, 0);
  assert.deepEqual(state.success, ["登录令牌已续期"]);
});

test("续期进行中重复点击不会产生第二个刷新请求", async () => {
  const state = fixture();
  let finish!: (account: CodexAccount) => void;
  state.context.refreshCodexAccountToken = async (id) => {
    state.calls.push(id);
    return new Promise((resolve) => { finish = resolve; });
  };
  const first = state.run(account);
  assert.equal(state.context.tokenRefreshingIds.value.includes(account.id), true);
  await state.run(account);
  assert.deepEqual(state.calls, [account.id]);
  finish(account);
  await first;
  assert.equal(state.context.tokenRefreshingIds.value.length, 0);
});

test("续期失败不误报成功、不修改到期时间，并允许重试", async () => {
  const state = fixture();
  state.context.refreshCodexAccountToken = async () => { throw "refresh_token_expired"; };
  await state.run(account);
  assert.deepEqual(state.success, []);
  assert.match(state.errors[0], /续期失败.*refresh_token_expired/);
  assert.equal(state.context.accounts.value[0], account);
  assert.equal(state.context.tokenRefreshingIds.value.length, 0);
  state.context.refreshCodexAccountToken = async () => account;
  await state.run(account);
  assert.equal(state.success.length, 1);
});

test("使用最新账号数据拒绝已删除或已移除续期凭据的账号", async () => {
  const state = fixture();
  state.context.accounts.value = [{ ...account, tokens: { ...account.tokens, refresh_token: "" } }];
  await state.run(account);
  state.context.accounts.value = [];
  await state.run(account);
  assert.deepEqual(state.calls, []);
});
