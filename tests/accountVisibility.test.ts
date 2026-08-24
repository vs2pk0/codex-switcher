import assert from "node:assert/strict";
import test from "node:test";

import {
  becameHiddenAccount,
  shouldCleanupHiddenAccount,
} from "../src/services/accountVisibility.ts";

test("只有账号首次保存成隐身模式时触发联动清理", () => {
  assert.equal(becameHiddenAccount(false, true), true);
  assert.equal(becameHiddenAccount(undefined, true), true);
  assert.equal(becameHiddenAccount(true, true), false);
  assert.equal(becameHiddenAccount(true, false), false);
  assert.equal(becameHiddenAccount(false, false), false);
});

test("清理失败可重试，但取消隐身时不再删除外部账号", () => {
  assert.equal(
    shouldCleanupHiddenAccount({
      previousHidden: false,
      nextHidden: true,
      nextPending: true,
    }),
    true,
  );
  assert.equal(
    shouldCleanupHiddenAccount({
      previousHidden: true,
      nextHidden: true,
      previousPending: true,
    }),
    true,
  );
  assert.equal(
    shouldCleanupHiddenAccount({
      previousHidden: true,
      nextHidden: false,
      previousPending: true,
    }),
    false,
  );
  assert.equal(
    shouldCleanupHiddenAccount({
      previousHidden: true,
      nextHidden: true,
    }),
    false,
  );
});
