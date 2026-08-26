import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const toolbarSource = readFileSync(
  new URL("../src/components/AccountToolbar.vue", import.meta.url),
  "utf8",
);
const appSource = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");
const apiServicePanelSource = readFileSync(
  new URL("../src/components/ApiServicePanel.vue", import.meta.url),
  "utf8",
);
const openCodexBackendSource = readFileSync(
  new URL("../src-tauri/src/opencodex/backend.rs", import.meta.url),
  "utf8",
);

test("账号工具栏通过统一绑定入口提供两个目标", () => {
  assert.match(toolbarSource, /t\("绑定"\)/);
  assert.match(toolbarSource, /value="api-service"/);
  assert.match(toolbarSource, /value="open-codex"/);
  assert.match(toolbarSource, /t\("绑定到 API 服务"\)/);
  assert.match(toolbarSource, /t\("绑定到 OpenCodex"\)/);
});

test("两个绑定目标都在写入前检查服务运行状态", () => {
  assert.match(appSource, /!serviceState\.service\.running/);
  assert.match(appSource, /!snapshot\.running/);
  const apiPanelBindHandler = apiServicePanelSource.slice(
    apiServicePanelSource.indexOf("async function openBindAccounts"),
    apiServicePanelSource.indexOf("async function bindSelectedAccounts"),
  );
  assert.match(apiPanelBindHandler, /!running\.value/);
  assert.match(apiPanelBindHandler, /请先启动 API 服务，再绑定账号/);
  assert.match(openCodexBackendSource, /running_open_codex_port\(\)/);
  assert.match(openCodexBackendSource, /stop_for_account_binding/);
});

test("OpenCodex 绑定按原运行方式恢复并覆盖部分停止失败", () => {
  assert.match(
    openCodexBackendSource,
    /AccountBindingRestartMode::BackgroundService[\s\S]*start_background_service_after_binding/,
  );
  assert.match(
    openCodexBackendSource,
    /AccountBindingRestartMode::Standalone[\s\S]*start_background\(/,
  );
  assert.match(
    openCodexBackendSource,
    /if let Err\(stop_error\)[\s\S]*probe_health\(port\)[\s\S]*restart_after_account_binding/,
  );
});
