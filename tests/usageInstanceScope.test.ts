import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const usagePanelSource = readFileSync(
  new URL("../src/components/UsagePanel.vue", import.meta.url),
  "utf8",
);
const usageServiceSource = readFileSync(
  new URL("../src/services/usage.ts", import.meta.url),
  "utf8",
);
const appSource = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");
const backendSource = readFileSync(
  new URL("../src-tauri/src/usage.rs", import.meta.url),
  "utf8",
);

test("消耗看板提供实例与全部实例范围并持久化选择", () => {
  assert.match(usagePanelSource, /codex-switcher:usage-instance-scope/);
  assert.match(usagePanelSource, /window\.localStorage\.setItem\(usageInstanceStorageKey, value\)/);
  assert.match(usagePanelSource, /<a-option :value="allInstancesScope">/);
  assert.match(usagePanelSource, /instanceDisplayName\(instance\)/);
  assert.match(appSource, /:instances="codexInstances"/);
  assert.match(appSource, /:current-instance-id="sessionInstanceId"/);
  assert.match(usagePanelSource, /readUsageInstanceScope\(props\.currentInstanceId\)/);
});

test("统计查询向后兼容默认实例并支持所有实例聚合", () => {
  assert.match(usageServiceSource, /instanceId: query\.instanceId \?\? null/);
  assert.match(usageServiceSource, /allInstances: query\.allInstances \?\? false/);
  assert.match(
    backendSource,
    /resolve_usage_targets\(instance_id\.as_deref\(\), all_instances\.unwrap_or\(false\)\)/,
  );
  assert.match(backendSource, /dedupe_usage_logs_preferred\(logs\)/);
  assert.match(backendSource, /usage_db_path_for_instance/);
});
