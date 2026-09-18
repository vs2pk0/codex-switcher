import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const appSource = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");
const toolbarSource = readFileSync(
  new URL("../src/components/AccountToolbar.vue", import.meta.url),
  "utf8",
);

test("首页工具栏仅在已选择账号时启用批量删除", () => {
  assert.match(toolbarSource, /selectedAccountCount: number/);
  assert.match(toolbarSource, /\(event: "batch-delete"\): void/);
  assert.match(
    toolbarSource,
    /class="batch-action batch-delete"[\s\S]*:disabled="selectedAccountCount === 0"[\s\S]*@click="\$emit\('batch-delete'\)"/,
  );
});

test("批量删除逐个处理账号，单个失败不会阻止后续删除", () => {
  assert.match(appSource, /function confirmBatchDelete\(\): void/);
  assert.match(
    appSource,
    /for \(const account of selected\)[\s\S]*await deleteCodexAccount\(account\.id\)[\s\S]*failures\.push/,
  );
  assert.match(appSource, /@batch-delete="confirmBatchDelete"/);
});
