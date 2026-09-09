import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";
import { test } from "node:test";

const read = (path: string) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");

test("客户端只携带运行时，不再依赖或提供内置 Engine 回退", () => {
  const manifest = JSON.parse(read("opencodex-engine/package.json"));
  const lock = JSON.parse(read("opencodex-engine/package-lock.json"));
  assert.ok(manifest.dependencies.bun);
  assert.equal(manifest.dependencies["@bitkyc08/opencodex"], undefined);
  assert.equal(lock.packages["node_modules/@bitkyc08/opencodex"], undefined);
  for (const file of ["src-tauri/src/opencodex/backend.rs", "src-tauri/src/opencodex/mod.rs", "src/opencodex/service.ts", "src/opencodex/OpenCodexPanel.vue"]) {
    assert.doesNotMatch(read(file), /activate_bundled_engine|activateBundledOpenCodexEngine|bundledVersion|bundled_package_root|回退内置版本/);
  }
});

test("实例、后台服务和账号转换均只加载后端指定的下载版本", () => {
  for (const file of ["manager-instance-integration.ts", "manager-service-status.ts", "manager-switcher-import.ts"]) {
    const source = read(`opencodex-engine/${file}`);
    assert.match(source, /manager-engine-package\.ts/);
    assert.doesNotMatch(source, /\.\/node_modules\/@bitkyc08/);
  }
  assert.match(read("src-tauri/src/opencodex/backend.rs"), /\.env\("OPENCODEX_PACKAGE_ROOT", package\)/);
});

test("Engine Worker 启动前就获得实例配置目录，服务源配置单独传递", () => {
  const backend = read("src-tauri/src/opencodex/backend.rs");
  assert.match(backend, /\.env\("OPENCODEX_HOME", integration_home\)/);
  assert.match(backend, /\.env\("OPENCODEX_MANAGER_SOURCE_HOME", source_home\)/);
  assert.match(read("opencodex-engine/manager-instance-integration.ts"), /process\.env\.OPENCODEX_MANAGER_SOURCE_HOME/);
});

test("无 Engine 时引导下载，首次激活不会要求旧 Engine 存在", () => {
  assert.match(read("src/opencodex/OpenCodexPanel.vue"), /if \(!snapshot\.value\?\.installed\)/);
  const backend = read("src-tauri/src/opencodex/backend.rs");
  assert.match(backend, /let old_launcher = match self\.active_launcher\(\)/);
  assert.match(backend, /run_background_service_helper_for\("status", &package\)/);
  assert.match(backend, /self\.write_active_engine\(Some\(version\)\)\?;/);
});
