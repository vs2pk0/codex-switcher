import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import test from "node:test";
import ts from "typescript";

// 执行实际文件导入入口，替换 IPC 与界面依赖，验证阈值和刷新调用。
const source = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");
const handler = source.slice(
  source.indexOf("async function handleFileImport("),
  source.indexOf("\nasync function handleApiKeyAdd("),
);
const compiled = ts.transpileModule(handler, {
  compilerOptions: { target: ts.ScriptTarget.ES2022 },
}).outputText;

function fixture(failAt = -1) {
  const calls = { imports: 0, loads: 0, refreshes: 0, refreshedAccounts: 0, warnings: 0 };
  const importing = { value: false };
  const addModalVisible = { value: true };
  const successes: string[] = [];
  const errors: string[] = [];
  const importFiles = runInNewContext(`${compiled}\nhandleFileImport`, {
    importing, addModalVisible,
    async importCodexFromJson(content: string) {
      const index = calls.imports++;
      if (index === failAt) throw new Error("invalid JSON");
      return JSON.parse(content);
    },
    async loadAccounts() { calls.loads++; },
    async refreshAccountsAfterMutation(accounts: unknown[]) {
      calls.refreshes++;
      calls.refreshedAccounts = accounts.length;
      return 0;
    },
    warnQuotaRefreshFailures(count: number) { calls.warnings += count; },
    formatTranslatedText(text: string, values: Record<string, number>) {
      return text.replace(/\{(\w+)\}/g, (_, key: string) => String(values[key]));
    },
    errorText: String,
    Message: {
      success(message: string) { successes.push(message); },
      error(message: string) { errors.push(message); },
    },
  }) as (files: Array<{ text(): Promise<string> }>) => Promise<void>;
  return { importFiles, calls, importing, addModalVisible, successes, errors };
}

function files(count: number, accountsPerFile = 1) {
  return Array.from({ length: count }, (_, file) => ({
    async text() {
      return JSON.stringify(Array.from({ length: accountsPerFile }, (_, account) => ({ id: `${file}-${account}` })));
    },
  }));
}

test("10 个 JSON 文件仍在导入完成后刷新", async () => {
  const state = fixture();
  await state.importFiles(files(10));
  assert.equal(state.calls.imports, 10);
  assert.equal(state.calls.loads, 1);
  assert.equal(state.calls.refreshes, 1);
  assert.equal(state.calls.refreshedAccounts, 10);
  assert.equal(state.importing.value, false);
  assert.equal(state.addModalVisible.value, false);
});

test("超过 10 个文件全部导入并更新列表，但不触发逐账号刷新", async () => {
  for (const count of [11, 100]) {
    const state = fixture();
    await state.importFiles(files(count));
    assert.equal(state.calls.imports, count);
    assert.equal(state.calls.loads, 1);
    assert.equal(state.calls.refreshes, 0);
    assert.equal(state.calls.warnings, 0);
    assert.equal(state.errors.length, 0);
    assert.match(state.successes[0]!, /已跳过自动刷新/);
    assert.equal(state.importing.value, false);
    assert.equal(state.addModalVisible.value, false);
  }
});

test("阈值按 JSON 文件数判断", async () => {
  const state = fixture();
  await state.importFiles(files(1, 11));
  assert.equal(state.calls.imports, 1);
  assert.equal(state.calls.refreshes, 1);
  assert.equal(state.calls.refreshedAccounts, 11);
});

test("空选择和导入失败不触发刷新，失败后解除导入状态", async () => {
  const empty = fixture();
  await empty.importFiles([]);
  assert.equal(empty.calls.imports, 0);
  assert.equal(empty.calls.loads, 0);
  assert.equal(empty.calls.refreshes, 0);
  const failed = fixture(2);
  await failed.importFiles(files(11));
  assert.equal(failed.calls.refreshes, 0);
  assert.equal(failed.successes.length, 0);
  assert.equal(failed.errors.length, 1);
  assert.equal(failed.importing.value, false);
});
