import assert from "node:assert/strict";
import test from "node:test";
import { creditBalance, formatCreditBalance, hasCreditBalance } from "../src/quota.ts";
import type { CodexQuota } from "../src/types/codex.ts";

function quota(raw_data: unknown): CodexQuota {
  return { hourly_percentage: 100, weekly_percentage: 100, raw_data };
}

test("从原有额度响应读取积分余额，保留小数与原始精度", () => {
  assert.deepEqual(creditBalance(quota({ credits: { has_credits: true, unlimited: false, balance: "1000.0000000000" } })),
    { balance: 1000, rawBalance: "1000.0000000000", unlimited: false });
  assert.equal(creditBalance(quota({ credits: { balance: "966.1407150000" } }))?.balance, 966.140715);
  assert.equal(formatCreditBalance(1000, "en-US"), "1,000");
  assert.equal(formatCreditBalance(966.140715, "en-US"), "966.14");
});

test("解析保留零余额，null 和缺失不会变成零", () => {
  for (const balance of [0, "0", "0.0000000000"]) {
    assert.equal(creditBalance(quota({ credits: { balance } }))?.balance, 0);
  }
  assert.equal(formatCreditBalance(0, "en-US"), "0");
  for (const balance of [null, undefined, "", "   ", true, [], {}, "NaN", "Infinity", "1,000", "0x10"]) {
    assert.equal(creditBalance(quota({ credits: { has_credits: false, balance } }))?.balance, undefined);
  }
});

test("零余额或未知余额隐藏积分行，有余额及无限额度继续显示", () => {
  for (const balance of [0, "0", "0.0000000000", null, undefined, "", "invalid"]) {
    assert.equal(hasCreditBalance(quota({ credits: { balance } })), false);
  }
  for (const value of [undefined, quota({}), quota({ credits: null })]) {
    assert.equal(hasCreditBalance(value), false);
  }
  for (const balance of [1000, "966.1407150000", "0.0001", -1]) {
    assert.equal(hasCreditBalance(quota({ credits: { balance } })), true);
  }
  assert.equal(hasCreditBalance(quota({ credits: { unlimited: true, balance: null } })), true);
});

test("无限额度优先由布尔标识确定，不把字符串 false 当成 true", () => {
  assert.equal(creditBalance(quota({ credits: { unlimited: true, balance: null } }))?.unlimited, true);
  assert.equal(creditBalance(quota({ credits: { unlimited: "false", balance: "100" } }))?.unlimited, false);
});

test("兼容 data 包装及数字余额，重置券数量不充当积分余额", () => {
  assert.equal(creditBalance(quota({ data: { credits: { balance: 25.5 } } }))?.balance, 25.5);
  assert.equal(creditBalance(quota({ rate_limit_reset_credits: { available_count: 1000 } })), undefined);
  assert.equal(creditBalance(quota({ credits: [{ amount: 1000 }] })), undefined);
  assert.equal(creditBalance(undefined), undefined);
});

test("小额积分不会因格式化被误显示为零，数值按语言格式化", () => {
  assert.equal(formatCreditBalance(0.0001, "en-US"), "< 0.01");
  assert.equal(formatCreditBalance(-0.0001, "en-US"), "> -0.01");
  assert.equal(formatCreditBalance(1234.5, "de-DE"), "1.234,5");
});
