import test from "node:test";
import assert from "node:assert/strict";
import {
  beginPendingItem,
  finishPendingItem,
  formatResetDateTime,
  hasAvailableResetCredit,
  resetCreditRowActionState,
} from "../src/services/resetUiState.ts";

interface CreditFixture {
  status: "available" | "used";
}

const isAvailable = (credit: CreditFixture) => credit.status === "available";

test("任意一条可用次数即可启用重置操作", () => {
  assert.equal(
    hasAvailableResetCredit(
      [{ status: "used" }, { status: "available" }],
      isAvailable,
    ),
    true,
  );
  assert.equal(hasAvailableResetCredit([{ status: "used" }], isAvailable), false);
});

test("同一预约取消处理中拒绝再次入队", () => {
  assert.deepEqual(beginPendingItem([], "schedule-1"), ["schedule-1"]);
  assert.equal(beginPendingItem(["schedule-1"], "schedule-1"), null);
});

test("取消结束后只移除对应预约 ID", () => {
  assert.deepEqual(
    finishPendingItem(["schedule-1", "schedule-2"], "schedule-1"),
    ["schedule-2"],
  );
});

test("可用记录提供账号级立即重置和预约入口", () => {
  assert.deepEqual(resetCreditRowActionState(true, false, false), {
    consumeDisabled: false,
    scheduleDisabled: false,
    scheduleAction: "create",
  });
});

test("已有预约时禁止立即重置并把预约入口改为查看", () => {
  assert.deepEqual(resetCreditRowActionState(true, true, false), {
    consumeDisabled: true,
    scheduleDisabled: false,
    scheduleAction: "view",
  });
});

test("不可用记录或忙碌状态禁止发起新操作", () => {
  assert.equal(resetCreditRowActionState(false, false, false).consumeDisabled, true);
  assert.equal(resetCreditRowActionState(false, false, false).scheduleDisabled, true);
  assert.equal(resetCreditRowActionState(true, false, true).consumeDisabled, true);
  assert.equal(resetCreditRowActionState(true, false, true).scheduleDisabled, true);
});

test("重置记录日期格式遵循当前界面语言", () => {
  const timestamp = new Date(2026, 7, 13, 0, 32, 0, 0).getTime();
  const simplifiedChinese = formatResetDateTime(timestamp, "zh-CN");
  const english = formatResetDateTime(timestamp, "en-US");
  const russian = formatResetDateTime(timestamp, "ru-RU");

  assert.notEqual(english, simplifiedChinese);
  assert.notEqual(russian, simplifiedChinese);
  assert.match(english, /2026/);
  assert.match(russian, /2026/);
});

test("重置记录日期遇到超出 Date 范围的时间戳时安全降级", () => {
  assert.equal(formatResetDateTime(9_000_000_000_000_000, "zh-CN"), "—");
});
