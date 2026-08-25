import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const backendSource = readFileSync(
  new URL("../src-tauri/src/lib.rs", import.meta.url),
  "utf8",
);
const appSource = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");

function rustCommandBody(name: string): string {
  const start = backendSource.indexOf(`fn ${name}(`);
  assert.notEqual(start, -1, `找不到后端命令 ${name}`);
  const nextCommand = backendSource.indexOf("\n#[tauri::command]", start + 1);
  return backendSource.slice(start, nextCommand === -1 ? undefined : nextCommand);
}

test("移入回收站不会停止或重新启动 Codex 实例", () => {
  const body = rustCommandBody("codex_move_sessions_to_trash_across_instances");
  assert.doesNotMatch(body, /run_with_instance_restarted|restart_codex_instance/);
  assert.match(body, /session_store_for_instance/);
});

test("复制会话不会停止或重新启动目标 Codex 实例", () => {
  const body = rustCommandBody("codex_copy_session_history_across_instances");
  assert.doesNotMatch(body, /run_with_instance_restarted|restart_codex_instance/);
  assert.match(body, /session_store_for_instance\(Some\(&target_instance_id\)\)/);
});

test("复制完成提示不再声称目标实例已自动重启", () => {
  const start = appSource.indexOf("async function handleCopySession(");
  const end = appSource.indexOf("\nfunction openSessionRename", start);
  assert.notEqual(start, -1, "找不到复制会话前端处理函数");
  assert.notEqual(end, -1, "找不到复制会话前端处理函数结尾");
  assert.doesNotMatch(appSource.slice(start, end), /自动重启/);
});
