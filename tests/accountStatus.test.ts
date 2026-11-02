import assert from "node:assert/strict";
import test from "node:test";
import { isAccountSubscriptionInactive, isSubscriptionExpired, subscriptionUntil } from "../src/accountStatus.ts";
import type { CodexAccount } from "../src/types/codex.ts";

const account: CodexAccount = {
  id: "fixture", email: "fixture@example.com", plan_type: "pro",
  tokens: { id_token: "", access_token: "" }, created_at: 0, last_used: 0,
};
const now = Date.parse("2026-11-01T00:00:00Z");

test("confirmed inactive subscription ignores cached future expiry and JWT fallback", () => {
  const inactive = { ...account, subscription_is_active: false, subscription_active_until: "2099-01-01T00:00:00Z" };
  assert.equal(isAccountSubscriptionInactive(inactive, now), true);
  assert.equal(subscriptionUntil(inactive, "2099-02-01T00:00:00Z"), undefined);
  assert.equal(isAccountSubscriptionInactive({ ...inactive, subscription_active_until: undefined }, now), true);
});

test("legacy accounts retain date checks and JWT fallback until refreshed", () => {
  assert.equal(isAccountSubscriptionInactive(account, now), false);
  assert.equal(subscriptionUntil(account, "2099-01-01T00:00:00Z"), "2099-01-01T00:00:00Z");
  assert.equal(isAccountSubscriptionInactive({ ...account, subscription_active_until: "2026-10-01T00:00:00Z" }, now), true);
  assert.equal(isSubscriptionExpired("invalid", now), false);
});

test("renewed subscriptions use refreshed expiry", () => {
  const renewed = { ...account, subscription_is_active: true, subscription_active_until: "2099-02-01T00:00:00Z" };
  assert.equal(isAccountSubscriptionInactive(renewed, now), false);
  assert.equal(subscriptionUntil(renewed, "2026-10-01T00:00:00Z"), renewed.subscription_active_until);
  assert.equal(subscriptionUntil({ ...renewed, subscription_active_until: undefined }, "2099-01-01T00:00:00Z"), undefined);
});
