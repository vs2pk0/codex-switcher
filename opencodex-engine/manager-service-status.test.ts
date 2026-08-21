import { afterEach, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setConfiguredServicePort } from "./manager-service-status.ts";

const temporaryRoots: string[] = [];
const originalOpenCodexHome = process.env.OPENCODEX_HOME;

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
