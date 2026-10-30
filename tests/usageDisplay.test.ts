import assert from "node:assert/strict";
import test from "node:test";
import { formatUsageTokens, normalizeTokenDisplayUnit, formatUsageSpeed, usageRequestSpeed, formatUnitPrice } from "../src/usageDisplay.ts";

test("session speed uses CC Switch thresholds and marks estimates", () => {
  assert.equal(usageRequestSpeed({ outputTokens: 520, latencyMs: 16_000, dataSource: "codex_session" }), "≈33 tok/s");
  assert.equal(usageRequestSpeed({ outputTokens: 199, latencyMs: 10_000, dataSource: "codex_session" }), "—");
  assert.equal(usageRequestSpeed({ outputTokens: 200, latencyMs: 999, dataSource: "codex_session" }), "—");
  assert.equal(usageRequestSpeed({ outputTokens: 200, dataSource: "codex_session" }), "—");
  assert.equal(usageRequestSpeed({ outputTokens: 1000, latencyMs: 12_000, firstTokenMs: 2000, dataSource: "proxy" }), "100 tok/s");
  assert.equal(usageRequestSpeed({ outputTokens: 1000, latencyMs: 12_000, dataSource: "proxy" }), "—");
  assert.equal(formatUsageSpeed(500, 1_000_000), "≈0.5 tok/s");
  assert.equal(formatUsageSpeed(0, 1000), "—");
});

test("unit prices retain small prices and use at least two decimals", () => {
  assert.equal(formatUnitPrice("10"), "$10.00");
  assert.equal(formatUnitPrice("0.075"), "$0.075");
  assert.equal(formatUnitPrice("0.0028"), "$0.0028");
  assert.equal(formatUnitPrice("0.00001"), "$0.00001");
  assert.equal(formatUnitPrice("0.000001"), "$0.000001");
  assert.equal(formatUnitPrice("0.00125"), "$0.00125");
  assert.equal(formatUnitPrice("1000"), "$1000.00");
});

test("K/M/B token display works in Chinese and preserves small counts", () => {
  for (const [count, expected] of [[0, "0"], [262, "262"], [171_234, "171.23K"], [326_000_000, "326M"], [1_250_000_000, "1.25B"]] as const) {
    assert.equal(formatUsageTokens(count, "compact", "zh-CN"), expected);
  }
  assert.equal(formatUsageTokens(999_999, "compact", "en-US"), "1M");
  assert.equal(formatUsageTokens(994_999, "compact", "en-US"), "995K");
  assert.equal(formatUsageTokens(999_994, "compact", "en-US"), "999.99K");
});

test("display modes change presentation without changing the original count", () => {
  const count = 326_123_456;
  assert.equal(formatUsageTokens(count, "localized", "zh-CN"), "3.26亿");
  assert.equal(formatUsageTokens(count, "localized", "zh-TW"), "3.26億");
  assert.equal(formatUsageTokens(123_456, "localized", "zh-TW"), "12.35萬");
  assert.equal(formatUsageTokens(count, "localized", "en-US"), "326.12M");
  assert.equal(formatUsageTokens(count, "full", "zh-CN"), "326,123,456");
  assert.equal(count, 326_123_456);
});

test("invalid persisted modes and absent data have safe defaults", () => {
  for (const value of [null, "", "M", {}, undefined]) assert.equal(normalizeTokenDisplayUnit(value), "compact");
  for (const value of ["localized", "full", "compact"] as const) assert.equal(normalizeTokenDisplayUnit(value), value);
  for (const value of [undefined, NaN, Infinity, -1]) assert.equal(formatUsageTokens(value, "compact", "zh-CN"), "0");
});
