import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("../src/components/UsagePanel.vue", import.meta.url), "utf8");

test("手动刷新重建统计，未解决的异常不会显示为全部成功", () => {
  assert.match(source, /function refreshUsage\(\)[\s\S]*?loadUsage\(true\)/);
  assert.match(source, /getCodexUsageDashboard\(\{[\s\S]*?refresh,/);
  assert.match(source, /if \(nextDashboard\.errors\.length\) \{\s*Message\.warning/);
  assert.match(source, /无法自动修复，请查看异常详情/);
});

test("统计告警展示可展开的错误详情并使用文本插值", () => {
  assert.match(source, /<details class="usage-warning-details">/);
  assert.match(source, /v-for="\(error, index\) in dashboard\.errors"[^>]*>\{\{ error \}\}/);
  assert.doesNotMatch(source, /v-html="error"/);
});
