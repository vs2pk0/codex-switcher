import test from "node:test";
import assert from "node:assert/strict";
import {
  formatLocalScheduleInput,
  parseLocalScheduleInput,
  resolveResetScheduleEntry,
} from "../src/services/resetScheduleEntry.ts";

test("没有活动预约时打开创建弹窗", () => {
  assert.equal(resolveResetScheduleEntry(false), "create");
});

test("存在活动预约时进入预约列表", () => {
  assert.equal(resolveResetScheduleEntry(true), "view");
});

test("严格解析本地分钟时间", () => {
  assert.equal(
    parseLocalScheduleInput("2026-08-13 00:32"),
    new Date(2026, 7, 13, 0, 32, 0, 0).getTime(),
  );
});

test("拒绝浏览器可能自动进位的无效日期", () => {
  assert.equal(parseLocalScheduleInput("2026-02-30 10:00"), null);
  assert.equal(parseLocalScheduleInput("2026-08-13T00:32"), null);
});

test("把时间戳格式化为本地分钟输入并可无损解析", () => {
  const timestamp = new Date(2026, 7, 13, 0, 32, 45, 999).getTime();
  assert.equal(formatLocalScheduleInput(timestamp), "2026-08-13 00:32");
  assert.equal(
    parseLocalScheduleInput(formatLocalScheduleInput(timestamp)),
    new Date(2026, 7, 13, 0, 32, 0, 0).getTime(),
  );
});

test("拒绝无效时间戳的格式化", () => {
  assert.equal(formatLocalScheduleInput(Number.NaN), "");
});
