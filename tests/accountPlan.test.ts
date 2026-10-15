import assert from "node:assert/strict";
import test from "node:test";
import { accountProPlan, normalizeAuthFilePlan, proPlanLabel } from "../src/accountPlan.ts";
import type { CodexAccount } from "../src/types/codex.ts";

function account(plan_type: string, auth_file_plan_type?: string): CodexAccount {
  return {
    id: "fixture", email: "fixture@example.com", plan_type, auth_file_plan_type,
    tokens: { id_token: "", access_token: "" }, created_at: 0, last_used: 0,
  };
}

test("订阅接口与 Token 中的 Pro Lite 别名都显示 5X", () => {
  for (const plan of ["chatgptprolite", "prolite", "chatgpt_pro_lite", "pro-5x", "codex-pro-5x"]) {
    assert.equal(normalizeAuthFilePlan(plan), "prolite");
    assert.equal(proPlanLabel(account(plan)), "PRO 5X");
  }
});

test("只有明确的 20X 档位才显示 20X", () => {
  for (const plan of ["promax", "chatgptpromax", "pro_20x", "codex-pro-20x"]) {
    assert.equal(proPlanLabel(account(plan)), "PRO 20X");
  }
  assert.equal(proPlanLabel(account("pro")), "PRO");
  assert.equal(proPlanLabel(account("chatgptpro")), "PRO");
});

test("细分凭据套餐优先，普通 pro 字段不遮挡已有细分套餐", () => {
  assert.equal(accountProPlan(account("pro", "prolite")), "prolite");
  assert.equal(accountProPlan(account("chatgptprolite", "pro")), "prolite");
  assert.equal(proPlanLabel(account("pro", "pro-20x")), "PRO 20X");
});
