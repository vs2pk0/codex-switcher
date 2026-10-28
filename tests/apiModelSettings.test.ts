import test from "node:test";
import assert from "node:assert/strict";
import { defaultApiModelSettings, normalizeApiModelSettings, validateApiModelSettings } from "../src/services/apiModelSettings.ts";

test("旧设置使用关闭的 API 偏好默认值，各份档位互不影响", () => {
  const original = normalizeApiModelSettings();
  assert.equal(original.autoCompactTokenLimit, 180_000);
  assert.equal(original.autoCompactEnabled, false);
  assert.equal(original.reasoningEnabled, false);
  assert.equal(original.disableWebSearch, false);
  assert.equal(original.fastMode, false);
  assert.equal(normalizeApiModelSettings({ disableWebSearch: true }).fastMode, false);
  const copied = normalizeApiModelSettings(original);
  copied.reasoningLevels.push("max");
  assert.equal(original.reasoningLevels.includes("max"), false);
});

test("API 偏好可序列化，并拒绝无效阈值、重复档位和未选中的默认强度", () => {
  const settings = { ...defaultApiModelSettings(), autoCompactEnabled: true, reasoningEnabled: true, disableWebSearch: true, fastMode: true };
  assert.deepEqual(normalizeApiModelSettings(JSON.parse(JSON.stringify(settings))), settings);
  assert.equal(validateApiModelSettings(settings), null);
  for (const limit of [0, -1, 1.5, NaN, Infinity, 2_147_483_648]) {
    assert.ok(validateApiModelSettings({ ...settings, autoCompactTokenLimit: limit }));
  }
  for (const levels of [[], ["high", "high"], ["Bad Name"], ["low"]]) {
    assert.ok(validateApiModelSettings({ ...settings, reasoningLevels: levels }));
  }
  assert.equal(validateApiModelSettings({ ...settings, reasoningLevels: ["low", "ultra"], reasoningEffort: "ultra" }), null);
});
