import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(
  new URL("../src-tauri/src/api_service.rs", import.meta.url),
  "utf8",
);

test("API 服务绑定在替换 Windows 认证目录前停服并在结束后重启", () => {
  const start = source.indexOf("pub async fn api_service_bind_accounts");
  const end = source.indexOf("pub fn api_service_list_bound_accounts", start);
  const handler = source.slice(start, end);
  const stopAt = handler.indexOf("stop_service_impl(&process)");
  const replaceAt = handler.indexOf("replace_auth_directory(&auth_dir, staging.path())");
  const restartAt = handler.lastIndexOf("start_service_impl(&app, &process, &download, &operation)");

  assert.ok(stopAt >= 0, "绑定前应停止 API 服务以释放 Windows 文件句柄");
  assert.ok(replaceAt > stopAt, "认证目录只能在服务停止后替换");
  assert.ok(restartAt > replaceAt, "绑定完成后应恢复 API 服务运行");
  assert.match(handler, /绑定账号前停止 API 服务失败/);
  assert.match(handler, /API 服务已恢复运行/);
  assert.match(handler, /恢复运行 API 服务失败/);
  assert.match(handler, /账号已写入 API 服务，但自动重新启动失败/);
});
