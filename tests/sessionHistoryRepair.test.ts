import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { buildSingleSessionHistoryRepairOptions } from "../src/services/session.ts";

const sessionPanelSource = readFileSync(
  new URL("../src/components/SessionPanel.vue", import.meta.url),
  "utf8",
);

test("单条会话修复固定使用深度模式并限制到当前实例和会话", () => {
  assert.deepEqual(
    buildSingleSessionHistoryRepairOptions(" session-1 ", {
      id: "instance-work",
      currentProvider: "api-key-provider",
    }),
    {
      mode: "deep",
      targetProvider: "api-key-provider",
      targetInstanceId: "instance-work",
      repairInstanceIds: ["instance-work"],
      sessionIds: ["session-1"],
    },
  );
});

test("会话操作区提供恢复完整会话按钮", () => {
  assert.match(sessionPanelSource, /t\('恢复完整会话'\)/);
  assert.match(sessionPanelSource, /emit\('repair-session-history', session\)/);
  assert.match(sessionPanelSource, /repairingSessionId === session\.id/);
});
