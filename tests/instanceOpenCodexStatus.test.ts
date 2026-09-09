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
const openCodexPanelSource = readFileSync(new URL("../src/opencodex/OpenCodexPanel.vue", import.meta.url), "utf8");

test("OpenCodex 常驻实例下拉为同步、恢复和图片模型传递相同目标", () => {
  assert.match(openCodexPanelSource, /v-model="selectedInstanceId"/);
  assert.match(openCodexPanelSource, /executeAction\(action, selectedInstanceId\.value\)/);
  assert.match(openCodexPanelSource, /updateOpenCodexVisionModels\(models, selectedInstanceId\.value\)/);
  assert.match(openCodexPanelSource, /selectedInstance\?\.openCodexConnected/);
  assert.doesNotMatch(openCodexBackendSource, /run_instance_integration_process\("isolate-default"/);
});

test("同步先重置基础配置再写入，成功后才打开实例；重置先停用集成", () => {
  assert.match(openCodexBackendSource, /run_with_instance_opened_on_success/);
  assert.match(openCodexBackendSource, /reset_instance_config_inner\(&instance.id\)\?;\s*self.run_instance_integration_helper/);
  assert.match(openCodexBackendSource, /run_instance_integration_process\("disable"[\s\S]*?reset_codex_config_for_instance/);
  assert.match(openCodexPanelSource, /service-control-card[\s\S]*?instance-target-bar[\s\S]*?health-indicator/);
});

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
  const helper = openCodexBackendSource.slice(
    openCodexBackendSource.indexOf("fn run_instance_integration_process("),
    openCodexBackendSource.indexOf("fn run_switcher_helper<"),
  );
  assert.match(
    helper,
    /let active_package_root = if action_name == "disable"/,
  );
  assert.match(
    helper,
    /else\s*\{\s*Some\(\s*self\.active_launcher\(\)\?\s*\.working_dir/,
  );
  assert.match(helper, /if let Some\(package\) = active_package_root\s*\{\s*command\.env\("OPENCODEX_PACKAGE_ROOT", package\)/);
  assert.match(helper, /command\.env_remove\("OPENCODEX_PACKAGE_ROOT"\)/);
});
