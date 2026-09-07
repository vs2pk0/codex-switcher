import assert from "node:assert/strict";
import test from "node:test";
import { filterMigrationAccounts, toggleVisibleAccounts } from "../src/opencodex/accounts.ts";

const accounts = [
  { sourceId: "alpha", targetAccountId: "target-a", email: "First@example.com", plan: "plus", current: false, eligible: true, deletable: false, status: "ready", reason: "" },
  { sourceId: "beta", targetAccountId: "target-b", email: "second@example.com", plan: "free", current: false, eligible: false, deletable: true, status: "already_imported", reason: "" },
  { sourceId: "gamma", targetAccountId: "target-c", email: "third@example.com", plan: null, current: false, eligible: false, deletable: false, status: "invalid", reason: "" },
];

test("account search matches email and both IDs with combined status and plan filters", () => {
  assert.deepEqual(filterMigrationAccounts(accounts, "  FIRST@EXAMPLE.COM ", "ready", "plus"), [accounts[0]]);
  assert.deepEqual(filterMigrationAccounts(accounts, "target-b", "", ""), [accounts[1]]);
  assert.deepEqual(filterMigrationAccounts(accounts, "alpha", "already_imported", ""), []);
  assert.deepEqual(filterMigrationAccounts(accounts, "", "", "__unknown__"), [accounts[2]]);
  assert.deepEqual(filterMigrationAccounts(accounts, "", "", ""), accounts);
});

test("select all only selects actionable visible accounts and preserves hidden selection", () => {
  assert.deepEqual(toggleVisibleAccounts(["beta"], [accounts[0], accounts[2]], true), ["beta", "alpha"]);
  assert.deepEqual(toggleVisibleAccounts(["alpha", "beta"], [accounts[0]], false), ["beta"]);
  assert.deepEqual(toggleVisibleAccounts(["alpha"], accounts, true), ["alpha", "beta"]);
  assert.deepEqual(toggleVisibleAccounts(["beta"], [], false), ["beta"]);
});
