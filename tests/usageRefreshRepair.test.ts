import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("../src/components/UsagePanel.vue", import.meta.url), "utf8");
const usageBackend = readFileSync(new URL("../src-tauri/src/usage.rs", import.meta.url), "utf8");

test("手动刷新重建统计，未归属的异常只记录到控制台而不打扰用户", () => {
  assert.match(source, /function refreshUsage\(\)[\s\S]*?loadUsage\(true\)/);
  assert.match(source, /getCodexUsageDashboard\(\{[\s\S]*?refresh,/);
  assert.match(source, /if \(nextDashboard\.errors\.length\) \{\s*\/\/[^\n]*\n\s*\/\/[^\n]*\n\s*console\.warn/);
  assert.doesNotMatch(source, /Message\.warning\(t\("消耗数据已刷新，仍有统计项无法自动修复/);
});

test("统计页不再渲染会话告警横幅", () => {
  assert.doesNotMatch(source, /usage-session-warning/);
  assert.doesNotMatch(source, /<details class="usage-warning-details">/);
  assert.doesNotMatch(source, /v-html="error"/);
});

test("刷新时自动归属子代理线程与独立计数 fork 的用量", () => {
  assert.match(usageBackend, /fn is_subagent_thread_meta\(payload: &Value\) -> bool/);
  assert.match(usageBackend, /owns_all_events_without_replay/);
  assert.match(usageBackend, /first_usage_starts_fresh_counter/);
  // 与父无公共前缀时，先尝试独立计数归属，再落到告警
  assert.match(
    usageBackend,
    /if lcp > 0 \{\s*Some\(lcp\)\s*\} else if file\.owns_all_events_without_replay \{/,
  );
  // 缓存版本提升，旧缓存会在下次打开统计时自动重建
  assert.match(usageBackend, /const USAGE_CACHE_VERSION: u32 = 8;/);
});
