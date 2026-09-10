import test from "node:test";
import assert from "node:assert/strict";
import {
  ADAPTIVE_COLUMNS,
  normalizeMaxColumns,
  resolveAccountColumns,
  resolveAdaptiveColumns,
} from "../src/services/accountLayout.ts";

test("每行账号数设置：非法值与 0 都归一化为自适应", () => {
  assert.equal(normalizeMaxColumns(0), ADAPTIVE_COLUMNS);
  assert.equal(normalizeMaxColumns(undefined), ADAPTIVE_COLUMNS);
  assert.equal(normalizeMaxColumns(7), ADAPTIVE_COLUMNS);
  assert.equal(normalizeMaxColumns("4"), 4);
  assert.equal(normalizeMaxColumns(5), 5);
});

test("自适应列数以 1800px 显示 5 列为基准", () => {
  assert.equal(resolveAdaptiveColumns(1800), 5);
  assert.equal(resolveAdaptiveColumns(1799), 4);
  assert.equal(resolveAdaptiveColumns(1440), 4);
  assert.equal(resolveAdaptiveColumns(2160), 6);
  // 窄窗口至少 3 列，超宽窗口有上限
  assert.equal(resolveAdaptiveColumns(900), 3);
  assert.equal(resolveAdaptiveColumns(10000), 8);
  assert.equal(resolveAdaptiveColumns(0), 5);
});

test("固定列数不受窗口宽度影响，自适应跟随窗口宽度", () => {
  assert.equal(resolveAccountColumns(3, 2400), 3);
  assert.equal(resolveAccountColumns(5, 900), 5);
  assert.equal(resolveAccountColumns(0, 1800), 5);
  assert.equal(resolveAccountColumns(0, 1200), 3);
});
