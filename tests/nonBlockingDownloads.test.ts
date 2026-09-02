import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const apiServiceSource = readFileSync(
  new URL("../src-tauri/src/api_service.rs", import.meta.url),
  "utf8",
);
const apiServicePanelSource = readFileSync(
  new URL("../src/components/ApiServicePanel.vue", import.meta.url),
  "utf8",
);
const openCodexCommandsSource = readFileSync(
  new URL("../src-tauri/src/opencodex/mod.rs", import.meta.url),
  "utf8",
);
const openCodexPanelSource = readFileSync(
  new URL("../src/opencodex/OpenCodexPanel.vue", import.meta.url),
  "utf8",
);

test("API 服务下载和更新检测不会阻塞 Tauri 主线程", () => {
  assert.match(apiServiceSource, /pub async fn api_service_download_update/);
  assert.match(apiServiceSource, /pub async fn api_service_check_update/);
  assert.match(apiServiceSource, /pub async fn api_service_start/);
  assert.match(apiServiceSource, /spawn_blocking\(move \|\|/);
  assert.match(apiServicePanelSource, /:loading="downloading"/);
  assert.match(apiServicePanelSource, /<a-progress :percent="progressPercent"/);
});

test("OpenCodex Engine 下载和版本检测使用后台阻塞线程与 Arco Loading", () => {
  assert.match(openCodexCommandsSource, /pub async fn opencodex_install_engine_version/);
  assert.match(openCodexCommandsSource, /pub async fn opencodex_get_engine_update_catalog/);
  assert.match(openCodexCommandsSource, /spawn_blocking\(move \|\| backend\.install_engine_version/);
  assert.match(openCodexPanelSource, /const installingVersion = ref\(""\)/);
  assert.match(openCodexPanelSource, /:loading="installingVersion === catalog\.latestStable\.version"/);
  assert.match(openCodexPanelSource, /installingVersion === selectedRelease\.version/);
});
