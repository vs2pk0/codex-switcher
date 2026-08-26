import { afterEach, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  restoreInstance,
  withIsolatedOpenCodexConfig,
} from "./manager-instance-integration.ts";

const temporaryRoots: string[] = [];
const originalOpenCodexHome = process.env.OPENCODEX_HOME;
const originalCodexHome = process.env.CODEX_HOME;

afterEach(() => {
  if (originalOpenCodexHome === undefined) delete process.env.OPENCODEX_HOME;
  else process.env.OPENCODEX_HOME = originalOpenCodexHome;
  if (originalCodexHome === undefined) delete process.env.CODEX_HOME;
  else process.env.CODEX_HOME = originalCodexHome;
  for (const root of temporaryRoots.splice(0)) rmSync(root, { recursive: true, force: true });
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
