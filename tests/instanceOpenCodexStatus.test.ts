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

test("同步先预检再停止实例，不重置配置或修复历史；显式重置先停用集成", () => {
  assert.match(openCodexBackendSource, /run_with_instance_opened_on_success/);
  assert.match(
    openCodexBackendSource,
    /run_instance_integration_process\("preflight", port, home\)\?;\s*crate::instances::run_with_instance_opened_on_success/,
  );
  const actionWorker = openCodexBackendSource.slice(openCodexBackendSource.indexOf("fn action_worker("), openCodexBackendSource.indexOf("fn run_instance_integration_helper("));
  assert.doesNotMatch(actionWorker, /one_click_repair_session_store/);
  assert.doesNotMatch(actionWorker, /reset_instance_config_inner\(&instance.id\)/);
  assert.match(openCodexPanelSource, /syncOverlay\.value = \{[\s\S]*?steps: syncOverlaySteps\(\)/);
  assert.match(openCodexPanelSource, /if \(event\.action === "sync"\) syncOverlay\.value = null;/);
  assert.match(openCodexBackendSource, /run_instance_integration_process\("disable"[\s\S]*?reset_codex_config_for_instance/);
  assert.match(openCodexPanelSource, /service-control-card[\s\S]*?instance-target-bar[\s\S]*?health-indicator/);
});

test("实例状态公开并展示 OpenCodex 接入标识", () => {
  assert.match(instanceServiceSource, /openCodexConnected: boolean/);
  assert.match(instancePanelSource, /instance\.openCodexConnected/);
  assert.match(instancePickerSource, /instance\.openCodexConnected/);
});

test("实例状态公开 API 服务接入标识，启动实例前先拉起依赖服务", () => {
  assert.match(instanceServiceSource, /apiServiceConnected: boolean/);
  assert.match(instanceServiceSource, /invoke\("launch_codex_instance_with_services", \{ instanceId \}\)/);
  assert.match(instancePanelSource, /instance\.apiServiceConnected/);
  assert.match(instancePanelSource, /startedServices = \(await launchCodexInstance\(instance\.id\)\)\.startedServices/);
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

const libSource = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
const apiServiceBackendSource = readFileSync(new URL("../src-tauri/src/api_service.rs", import.meta.url), "utf8");
const apiServicePanelSource = readFileSync(new URL("../src/components/ApiServicePanel.vue", import.meta.url), "utf8");

test("实例选择弹窗与实例卡片同步展示 API 服务接入标识", () => {
  assert.match(instancePickerSource, /instance\.apiServiceConnected/);
});

test("切换账号前对已接入服务或第三方 Provider 的实例先重置 config.toml", () => {
  const switchCommand = libSource.slice(
    libSource.indexOf("async fn switch_codex_account("),
    libSource.indexOf("pub(crate) fn switch_account_and_sync_session_provider("),
  );
  assert.match(switchCommand, /if codex_home_needs_config_reset_before_switch\(&codex_home\)\s*\{\s*reset_codex_config_for_instance\(&instance\.id\)\?;/);
  assert.ok(
    switchCommand.indexOf("reset_codex_config_for_instance") < switchCommand.indexOf("switch_account_and_sync_session_provider(&account_store"),
    "必须先重置 config.toml，再导入账号数据",
  );
  assert.match(libSource, /codex_home_has_opencodex_routing\(codex_home\)\s*\|\|\s*api_service::codex_home_has_api_service_routing\(codex_home\)/);
});

test("OpenCodex 同步保留账号，API 服务仍自动绑定 OAuth，两个面板保留手动绑定入口", () => {
  const syncStart = openCodexBackendSource.indexOf("run_with_instance_opened_on_success(&instance.id");
  const syncBlock = openCodexBackendSource.slice(
    syncStart,
    openCodexBackendSource.indexOf("} else if matches!(action, CommandAction::Start)", syncStart),
  );
  const helperIndex = syncBlock.indexOf("run_instance_integration_helper(&action, port, home)");
  const bindIndex = syncBlock.indexOf("prepare_default_oauth_account_blocking(");
  const repairIndex = syncBlock.indexOf("one_click_repair_session_store(&session_store");
  assert.ok(helperIndex >= 0);
  assert.equal(bindIndex, -1);
  assert.equal(repairIndex, -1);

  // API 服务：先挑选并续期 OAuth 账号，再在同步闭包里绑定到「本地 API 服务」账号
  const apiSync = apiServiceBackendSource.slice(
    apiServiceBackendSource.indexOf("pub async fn api_service_sync_codex_instance("),
    apiServiceBackendSource.indexOf("pub async fn api_service_restore_codex_instance("),
  );
  assert.match(apiSync, /prepare_default_oauth_account\(&instance_id\)\.await\?/);
  assert.match(apiSync, /auto_bind_default_oauth\(\s*crate::service_binding::ServiceKind::ApiService/);
  assert.ok(apiSync.indexOf("auto_bind_default_oauth") < apiSync.indexOf("switch_account_and_sync_session_provider("));

  // 两个面板都提供「绑定 OAuth」按钮并交给 App 复用 OAuth 绑定弹窗
  assert.match(openCodexPanelSource, /emit\("bind-oauth", \{\s*kind: "opencodex"/);
  assert.match(apiServicePanelSource, /emit\("bind-oauth", \{ kind: "api-service"/);
  assert.match(apiServicePanelSource, /openCodexIntegration\('bind-oauth'\)/);
});
