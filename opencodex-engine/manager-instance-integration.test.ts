import { afterEach, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import {
  restoreInstance,
  resolveOpenCodexPackageRoot,
  syncInstance,
  withIsolatedOpenCodexConfig,
} from "./manager-instance-integration.ts";

const temporaryRoots: string[] = [];
const originalOpenCodexHome = process.env.OPENCODEX_HOME;
const originalCodexHome = process.env.CODEX_HOME;
const originalOpenCodexPackageRoot = process.env.OPENCODEX_PACKAGE_ROOT;

afterEach(() => {
  if (originalOpenCodexHome === undefined) delete process.env.OPENCODEX_HOME;
  else process.env.OPENCODEX_HOME = originalOpenCodexHome;
  if (originalCodexHome === undefined) delete process.env.CODEX_HOME;
  else process.env.CODEX_HOME = originalCodexHome;
  if (originalOpenCodexPackageRoot === undefined) delete process.env.OPENCODEX_PACKAGE_ROOT;
  else process.env.OPENCODEX_PACKAGE_ROOT = originalOpenCodexPackageRoot;
  for (const root of temporaryRoots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function writeActiveEnginePackage(root: string): void {
  const codexDir = join(root, "src", "codex");
  mkdirSync(codexDir, { recursive: true });
  writeFileSync(join(root, "src", "config.ts"), [
    "export function loadConfig() { return {}; }",
    "export function applyProxyEnv() {}",
    "",
  ].join("\n"));
  writeFileSync(join(codexDir, "inject.ts"), [
    'import { writeFileSync } from "node:fs";',
    'import { join } from "node:path";',
    "export async function injectCodexConfig(port: number, _config: unknown, options: { validateOnly?: boolean }) {",
    "  if (!options.validateOnly) writeFileSync(join(process.env.CODEX_HOME!, \"active-engine-marker\"), String(port));",
    "  return { success: true, message: \"active engine package used\" };",
    "}",
    "export async function restoreNativeCodexAsync() { return { success: true, message: \"restored\", artifacts: {} }; }",
    "",
  ].join("\n"));
  writeFileSync(join(codexDir, "refresh.ts"), [
    "export async function refreshCodexModelCatalog() { return { catalogExists: false, path: \"\" }; }",
    "",
  ].join("\n"));
  writeFileSync(join(codexDir, "desired-state.ts"), [
    "export function setCodexIntegrationEnabled() { return { ok: true }; }",
    "",
  ].join("\n"));
}

test("多开同步优先加载当前激活的 OpenCodex Engine 包，并兼容回退内置包", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-instance-active-source-"));
  const target = mkdtempSync(join(tmpdir(), "opencodex-instance-active-target-"));
  const activePackage = mkdtempSync(join(tmpdir(), "opencodex-instance-active-package-"));
  temporaryRoots.push(source, target, activePackage);
  writeFileSync(join(source, "config.json"), JSON.stringify({ clientIntegrations: {} }));
  writeActiveEnginePackage(activePackage);
  process.env.OPENCODEX_HOME = source;
  process.env.CODEX_HOME = target;
  process.env.OPENCODEX_PACKAGE_ROOT = activePackage;

  const result = await syncInstance(15800);

  expect(result).toEqual({ action: "sync", success: true, message: "active engine package used" });
  expect(readFileSync(join(target, "active-engine-marker"), "utf8")).toBe("15800");
  expect(resolveOpenCodexPackageRoot(join(activePackage, "missing"))).toBe(resolve(
    import.meta.dir,
    "node_modules",
    "@bitkyc08",
    "opencodex",
  ));
});

test("多开实例同步只修改临时集成状态，不污染真实 OpenCodex 配置", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-instance-source-"));
  temporaryRoots.push(source);
  const original = {
    port: 15800,
    clientIntegrations: { codex: false, grok: true },
    providers: { demo: { apiKey: "secret" } },
  };
  const configPath = join(source, "config.json");
  writeFileSync(configPath, JSON.stringify(original));

  await withIsolatedOpenCodexConfig(source, true, async (overlay) => {
    expect(process.env.OPENCODEX_HOME).toBe(overlay);
    const isolated = JSON.parse(readFileSync(join(overlay, "config.json"), "utf8"));
    expect(isolated.clientIntegrations).toEqual({ codex: true, grok: true });
    expect(isolated.providers.demo.apiKey).toBe("secret");
  });

  expect(JSON.parse(readFileSync(configPath, "utf8"))).toEqual(original);
  expect(process.env.OPENCODEX_HOME).toBe(originalOpenCodexHome);
});

test("临时 OpenCodex 配置在操作结束后立即删除", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-instance-cleanup-"));
  temporaryRoots.push(source);
  writeFileSync(join(source, "config.json"), JSON.stringify({ clientIntegrations: {} }));
  let overlay = "";

  await withIsolatedOpenCodexConfig(source, false, async (path) => {
    overlay = path;
    expect(existsSync(path)).toBe(true);
  });

  expect(existsSync(overlay)).toBe(false);
  expect(JSON.parse(readFileSync(join(source, "config.json"), "utf8"))).toEqual({
    clientIntegrations: {},
  });
});

test("多开实例操作失败时仍清理临时令牌文件并恢复环境", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-instance-failure-"));
  temporaryRoots.push(source);
  writeFileSync(join(source, "config.json"), JSON.stringify({ clientIntegrations: {} }));
  writeFileSync(join(source, "auth.json"), JSON.stringify({ token: "secret" }));
  let overlay = "";

  await expect(withIsolatedOpenCodexConfig(source, true, async (path) => {
    overlay = path;
    throw new Error("expected failure");
  })).rejects.toThrow("expected failure");

  expect(existsSync(overlay)).toBe(false);
  expect(process.env.OPENCODEX_HOME).toBe(originalOpenCodexHome);
  expect(readFileSync(join(source, "auth.json"), "utf8")).toContain("secret");
});

test("多开实例恢复使用指定 CODEX_HOME 且保留真实全局开关", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-instance-restore-source-"));
  const target = mkdtempSync(join(tmpdir(), "opencodex-instance-restore-target-"));
  temporaryRoots.push(source, target);
  const globalConfig = {
    port: 15800,
    clientIntegrations: { codex: true },
    defaultProvider: "ollama",
    providers: {
      ollama: { baseUrl: "http://127.0.0.1:11434/v1", authMode: "local", adapter: "openai" },
    },
  };
  const globalConfigPath = join(source, "config.json");
  writeFileSync(globalConfigPath, JSON.stringify(globalConfig));
  writeFileSync(join(target, "config.toml"), 'model_provider = "openai"\n');
  process.env.OPENCODEX_HOME = source;
  process.env.CODEX_HOME = target;

  const result = await restoreInstance();

  expect(result.success).toBe(true);
  expect(JSON.parse(readFileSync(globalConfigPath, "utf8"))).toEqual(globalConfig);
  expect(readFileSync(join(target, "config.toml"), "utf8")).toContain('model_provider = "openai"');
});

test("多开实例同步前持久关闭默认集成并恢复主实例原生路由", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-instance-isolate-source-"));
  const target = mkdtempSync(join(tmpdir(), "opencodex-instance-isolate-target-"));
  temporaryRoots.push(source, target);
  const globalConfigPath = join(source, "config.json");
  writeFileSync(globalConfigPath, JSON.stringify({
    port: 15800,
    clientIntegrations: { grok: true },
    defaultProvider: "ollama",
    providers: {
      ollama: { baseUrl: "http://127.0.0.1:11434/v1", authMode: "local", adapter: "openai" },
    },
  }));
  writeFileSync(
    join(target, "config.toml"),
    [
      'model = "gpt-5.5"',
      "# Auto-injected by opencodex",
      'openai_base_url = "http://127.0.0.1:15800/v1"',
      "",
    ].join("\n"),
  );
  const execution = spawnSync(
    process.execPath,
    [join(import.meta.dir, "manager-instance-integration.ts"), "isolate-default", "15800"],
    {
      encoding: "utf8",
      env: {
        ...process.env,
        OPENCODEX_HOME: source,
        CODEX_HOME: target,
      },
    },
  );

  expect(execution.status).toBe(0);
  expect(JSON.parse(execution.stdout).success).toBe(true);
  expect(JSON.parse(readFileSync(globalConfigPath, "utf8")).clientIntegrations).toEqual({
    grok: true,
    codex: false,
  });
  expect(readFileSync(join(target, "config.toml"), "utf8")).not.toContain("openai_base_url");
});
