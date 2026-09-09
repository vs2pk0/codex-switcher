import { afterEach, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readServiceReferences, restoreDisabledAutostart, setConfiguredServicePort } from "./manager-service-status.ts";
type ServiceDiagnostic = { installed: boolean; conflict: boolean; backend: string; enabled: boolean };

const temporaryRoots: string[] = [];
const originalOpenCodexHome = process.env.OPENCODEX_HOME;

test("恢复 systemd 自启设置不会停止运行中的服务", () => {
  let enabled = true;
  const diagnose = () => ({ installed: true, conflict: false, backend: "systemd", enabled } as ServiceDiagnostic);
  const calls: string[][] = [];
  restoreDisabledAutostart(diagnose, (command, args) => { calls.push([command, ...args]); enabled = false; });
  expect(calls).toEqual([["systemctl", "--user", "disable", "opencodex-proxy"]]);
});

test("后台服务类型变化或自启设置恢复失败不会静默成功", () => {
  const state = { installed: true, conflict: false, backend: "systemd", enabled: true } as ServiceDiagnostic;
  expect(() => restoreDisabledAutostart(() => state, () => {})).toThrow("未能恢复");
  for (const invalid of [{ ...state, installed: false }, { ...state, conflict: true }, { ...state, backend: "launchd" as const }]) {
    expect(() => restoreDisabledAutostart(() => invalid, () => { throw new Error("must not run"); })).toThrow("类型已变化");
  }
});

test("后台服务引用合并去重，缺失或损坏的状态不会放行删除", () => {
  const root = mkdtempSync(join(tmpdir(), "opencodex-manager-references-"));
  temporaryRoots.push(root);
  const first = join(root, "first.json");
  const second = join(root, "second.json");
  const missing = join(root, "missing.json");
  const cliPath = join(root, "2.45.0", "src", "cli", "index.ts");
  writeFileSync(first, JSON.stringify({ cliPath }));
  writeFileSync(second, JSON.stringify({ cliPath }));
  expect(readServiceReferences([first, first, second, missing])).toEqual({ referencedCliPaths: [cliPath], referencesUnknown: false });
  expect(readServiceReferences([missing])).toEqual({ referencedCliPaths: [], referencesUnknown: true });
  for (const malformed of ["{", "null", "{}", JSON.stringify({ cliPath: " " })]) {
    writeFileSync(second, malformed);
    expect(readServiceReferences([first, second]).referencesUnknown).toBe(true);
  }
});

afterEach(() => {
  if (originalOpenCodexHome === undefined) delete process.env.OPENCODEX_HOME;
  else process.env.OPENCODEX_HOME = originalOpenCodexHome;
  for (const root of temporaryRoots.splice(0)) rmSync(root, { recursive: true, force: true });
});

test("后台服务注册前会把管理器端口写入 OpenCodex 配置", () => {
  const root = mkdtempSync(join(tmpdir(), "opencodex-manager-service-"));
  temporaryRoots.push(root);
  process.env.OPENCODEX_HOME = root;
  writeFileSync(join(root, "config.json"), JSON.stringify({
    port: 10100,
    defaultProvider: "ollama",
    providers: {
      ollama: { baseUrl: "http://127.0.0.1:11434/v1", authMode: "local", adapter: "openai" },
    },
  }));

  expect(setConfiguredServicePort(15800)).toEqual({ port: 15800 });
  expect(JSON.parse(readFileSync(join(root, "config.json"), "utf8")).port).toBe(15800);
  expect(() => setConfiguredServicePort(1023)).toThrow("端口必须在 1024–65535 之间");
});
