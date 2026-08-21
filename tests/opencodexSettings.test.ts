import assert from "node:assert/strict";
import test from "node:test";
import {
  DEFAULT_OPEN_CODEX_PORT,
  isOpenCodexPortPrompt,
  normalizeOpenCodexSettings,
  serializeOpenCodexSettings,
} from "../src/opencodex/settings.ts";

test("OpenCodex 新安装和旧版设置都迁移到 15800 默认端口", () => {
  assert.equal(DEFAULT_OPEN_CODEX_PORT, 15800);
  assert.deepEqual(normalizeOpenCodexSettings(undefined), {
    port: 15800,
    dashboardOpenMode: "client",
  });
  assert.deepEqual(normalizeOpenCodexSettings({ port: 10100, dashboardOpenMode: "browser" }), {
    port: 15800,
    dashboardOpenMode: "browser",
  });
});

test("OpenCodex 保存后的用户端口继续保留", () => {
  const saved = JSON.parse(serializeOpenCodexSettings({
    port: 16800,
    dashboardOpenMode: "browser",
  }));
  assert.deepEqual(normalizeOpenCodexSettings(saved), {
    port: 16800,
    dashboardOpenMode: "browser",
  });
});

test("OpenCodex 初始化端口提示可由管理器自动填写", () => {
  assert.equal(isOpenCodexPortPrompt("Proxy port [10100]: "), true);
  assert.equal(isOpenCodexPortPrompt("Provider name: "), false);
});
