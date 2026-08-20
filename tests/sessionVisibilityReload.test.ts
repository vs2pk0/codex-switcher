import assert from "node:assert/strict";
import test from "node:test";
import { reloadCodexAfterSessionVisibilityRepair } from "../src/services/codex.ts";

test("目录修复要求重载时会请求重启 ChatGPT/Codex", async () => {
  let restartCount = 0;

  const message = await reloadCodexAfterSessionVisibilityRepair(
    {
      scanned: 19,
      repaired: 19,
      message: "目录已修复",
      desktopReloadRequired: true,
    },
    async () => {
      restartCount += 1;
      return "已请求重启 ChatGPT/Codex";
    },
  );

  assert.equal(restartCount, 1);
  assert.equal(message, "已请求重启 ChatGPT/Codex");
});

test("没有会话需要重载时不会重启 ChatGPT/Codex", async () => {
  let restartCount = 0;

  const message = await reloadCodexAfterSessionVisibilityRepair(
    {
      scanned: 0,
      repaired: 0,
      message: "没有匹配的会话",
      desktopReloadRequired: false,
    },
    async () => {
      restartCount += 1;
      return "不应调用";
    },
  );

  assert.equal(restartCount, 0);
  assert.equal(message, null);
});
