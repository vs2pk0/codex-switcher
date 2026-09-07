import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const instanceServiceSource = readFileSync(
  new URL("../src/services/instances.ts", import.meta.url),
  "utf8",
);
const instancePanelSource = readFileSync(
  new URL("../src/components/CodexInstancesPanel.vue", import.meta.url),
  "utf8",
);
const instancePickerSource = readFileSync(
  new URL("../src/components/InstancePickerModal.vue", import.meta.url),
  "utf8",
);
const openCodexBackendSource = readFileSync(
  new URL("../src-tauri/src/opencodex/backend.rs", import.meta.url),
  "utf8",
);

test("实例状态公开并展示 OpenCodex 接入标识", () => {
  assert.match(instanceServiceSource, /openCodexConnected: boolean/);
  assert.match(instancePanelSource, /instance\.openCodexConnected/);
  assert.match(instancePickerSource, /instance\.openCodexConnected/);
});

test("实例卡片和选择弹窗保持等高及状态列对齐", () => {
  assert.match(instancePanelSource, /grid-template-rows: auto 1fr auto/);
  assert.match(instancePanelSource, /grid-template-columns: auto minmax\(0, 1fr\) auto/);
  assert.match(instancePickerSource, /grid-template-columns: minmax\(0, 1fr\) 156px/);
  assert.match(instancePickerSource, /\.instance-picker-runtime[^}]*width: 156px/);
  assert.doesNotMatch(instancePickerSource, /\.instance-picker-option span\s*\{/);
});

test("多开同步将服务当前激活的 Engine 包根目录传给集成 helper", () => {
  assert.match(
    openCodexBackendSource,
    /let active_package_root = self\s*\.active_launcher\(\)\?\s*\.working_dir/,
  );
  assert.match(
    openCodexBackendSource,
    /\.env\("OPENCODEX_PACKAGE_ROOT", &active_package_root\)/,
  );
});
