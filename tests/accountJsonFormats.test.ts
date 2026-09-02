import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const appSource = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");
const editModalSource = readFileSync(
  new URL("../src/components/EditAccountModal.vue", import.meta.url),
  "utf8",
);
const exportModalSource = readFileSync(
  new URL("../src/components/ExportJsonModal.vue", import.meta.url),
  "utf8",
);

test("账号编辑器分别展示 Switcher JSON 与 accounts 首个 Token JSON", () => {
  assert.match(editModalSource, /key="switcher-json"[^>]*:title="t\('Switcher JSON'\)"/);
  assert.match(editModalSource, /key="token-json"[^>]*:title="t\('Token JSON'\)"/);
  assert.match(appSource, /exportCodexAccounts\(\[account\.id\], "switcher_json"\)/);
  assert.match(appSource, /exportCodexAccounts\(\[account\.id\], "token_json"\)/);
  assert.match(appSource, /editTab\.value === "token-json"[\s\S]*editTokenJsonText\.value/);
});

test("单个和批量导出都提供 Switcher JSON 与 Token JSON 格式", () => {
  assert.match(appSource, /label: "Switcher JSON", value: "switcher_json"/);
  assert.match(appSource, /label: "Token JSON", value: "token_json"/);
  assert.match(
    appSource,
    /batchExportText\.value = await exportCodexAccounts\(ids, exportFormat\.value\)/,
  );
  assert.match(exportModalSource, /t\(option\.label\)/);
});
