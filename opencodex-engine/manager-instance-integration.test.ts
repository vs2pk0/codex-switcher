import { afterEach, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import {
  restoreInstance,
  isUsableModelCatalog,
  disableInstance,
  resolveOpenCodexPackageRoot,
  syncInstance,
  preflightInstance,
  withHistoryDatabase,
  seedEmptyCatalogFromEngine,
  repairNativeAuthAccountId,
  withIsolatedOpenCodexConfig,
} from "./manager-instance-integration.ts";

const temporaryRoots: string[] = [];
const originalOpenCodexHome = process.env.OPENCODEX_HOME;
const originalCodexHome = process.env.CODEX_HOME;
const originalOpenCodexPackageRoot = process.env.OPENCODEX_PACKAGE_ROOT;
const originalDefaultInstance = process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE;

test("关闭实例的 WAL 数据库可以预检且不修改会话", async () => {
  const root = mkdtempSync(join(tmpdir(), "opencodex-wal-preflight-"));
  temporaryRoots.push(root);
  const path = join(root, "state_5.sqlite");
  const db = new Database(join(root, "source.sqlite"));
  db.exec("PRAGMA journal_mode=WAL; CREATE TABLE threads(id TEXT, history_mode TEXT); INSERT INTO threads VALUES ('kept', 'paginated');");
  db.exec("PRAGMA wal_checkpoint(TRUNCATE)");
  db.close();
  writeFileSync(path, readFileSync(join(root, "source.sqlite")));
  const before = readFileSync(path);
  expect(existsSync(path + "-shm")).toBe(false);
  await withHistoryDatabase(path, async () => {
    const reader = new Database(path, { readonly: true });
    try { expect(reader.query("SELECT * FROM threads").all()).toEqual([{id:"kept",history_mode:"paginated"}]); }
    finally { reader.close(); }
  });
  expect(readFileSync(path)).toEqual(before);
  const missing = join(root, "missing.sqlite");
  await withHistoryDatabase(missing, async () => {});
  expect(existsSync(missing)).toBe(false);
  const invalid = join(root, "invalid.sqlite");
  writeFileSync(invalid, "invalid database");
  let called = false;
  await expect(withHistoryDatabase(invalid, async () => { called = true; })).rejects.toThrow();
  expect(called).toBe(false);
});

test("已下载 Engine 在隔离实例中同步非空目录并可恢复", async () => {
  const realEngine = resolveOpenCodexPackageRoot(originalOpenCodexPackageRoot);
  const source = mkdtempSync(join(tmpdir(), "opencodex-real-source-"));
  const target = mkdtempSync(join(tmpdir(), "opencodex-real-target-"));
  const defaultTarget = mkdtempSync(join(tmpdir(), "opencodex-real-default-"));
  const other = mkdtempSync(join(tmpdir(), "opencodex-real-other-"));
  temporaryRoots.push(source, target, defaultTarget, other);
  const sourceConfig = JSON.stringify({port:15800, clientIntegrations:{codex:false},
    defaultProvider:"demo", providers:{demo:{baseUrl:"http://127.0.0.1:1/v1",allowPrivateNetwork:true,adapter:"openai-responses",authMode:"local",models:["fixture-model"]}}});
  writeFileSync(join(source,"config.json"),sourceConfig);
  writeFileSync(join(target,"config.toml"),'model_provider = "openai"\n');
  writeFileSync(join(defaultTarget,"config.toml"),'model_provider = "openai"\n');
  writeFileSync(join(other,"config.toml"),'model_provider = "openai"\n');
  const token = `e30.${Buffer.from(JSON.stringify({exp:4102444800,"https://api.openai.com/auth":{chatgpt_account_id:"fixture-account"}})).toString("base64url")}.fixture`;
  const nativeAuth = JSON.stringify({tokens:{access_token:token,id_token:token,refresh_token:"fixture-refresh"},last_refresh:"unchanged"});
  writeFileSync(join(target,"auth.json"),nativeAuth);
  const diagnostics = spawnSync(process.execPath, ["--eval", `import { readConfigDiagnostics } from ${JSON.stringify(join(realEngine,"src/config.ts"))}; console.log(JSON.stringify(readConfigDiagnostics()));`], {encoding:"utf8",env:{...process.env,OPENCODEX_HOME:source,CODEX_HOME:defaultTarget}});
  const checked = JSON.parse(diagnostics.stdout);
  expect({source:checked.source,error:checked.error ?? ""}).toEqual({source:"file",error:""});
  const run = async (action: string, home = target) => {
    const code = `
      globalThis.fetch=async()=>{throw new Error("test network blocked")};
      for(const name of ["node:http","node:https"]){const m=(await import(name)).default;m.request=()=>{throw new Error("test network blocked")};m.get=m.request;}
      (await import("node:module")).syncBuiltinESMExports();
      const helper=await import(${JSON.stringify(join(import.meta.dir,"manager-instance-integration.ts"))});
      const result=await (${JSON.stringify(action)} === "sync" ? helper.syncInstance(15800) : helper.restoreInstance());
      console.log(JSON.stringify(result)); if(!result.success) process.exitCode=1;
    `;
    const child = Bun.spawn([process.execPath, "--eval", code], {
      env:{...process.env,OPENCODEX_HOME:home === defaultTarget ? source : join(home,".switcher-opencodex"),OPENCODEX_MANAGER_SOURCE_HOME:source,CODEX_HOME:home,OPENCODEX_PACKAGE_ROOT:realEngine,OPENCODEX_MANAGER_DEFAULT_INSTANCE:home === defaultTarget ? "1" : "0"},
      stdout:"pipe",stderr:"pipe",
    });
    const timeout = setTimeout(() => child.kill(), 20000);
    try {
      const [out,err,code] = await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);
      expect({code,detail:code === 0 ? "" : out+err}).toEqual({code:0,detail:""});
      if (action === "sync") expect(out).not.toContain("integration is disabled");
    } finally {clearTimeout(timeout);}
  };
  // The shared default intent is OFF: Workers must still see the selected
  // custom instance's ON state, including imports before their first message.
  await run("sync");
  expect(JSON.parse(readFileSync(join(target,"auth.json"),"utf8")).tokens.account_id).toBe("fixture-account");
  const authBackups = readdirSync(target).filter(name=>name.startsWith("auth.json.account-id-") && name.endsWith(".bak"));
  expect(authBackups.length).toBe(1);
  expect(readFileSync(join(target,authBackups[0]),"utf8")).toBe(nativeAuth);
  await run("sync", defaultTarget);
  const defaultConfig = readFileSync(join(defaultTarget,"config.toml"),"utf8");
  const sharedConfig = readFileSync(join(source,"config.json"),"utf8");
  await run("sync");
  expect(isUsableModelCatalog(join(target,"opencodex-catalog.json"))).toBe(true);
  expect(readFileSync(join(target,"opencodex-catalog.json"),"utf8")).toContain("fixture-model");
  expect(readFileSync(join(target,"config.toml"),"utf8")).toContain("15800");
  await run("sync", other);
  const otherConfig = readFileSync(join(other,"config.toml"),"utf8");
  await run("restore");
  expect(readFileSync(join(target,"config.toml"),"utf8")).not.toContain("15800");
  await run("sync");
  expect(isUsableModelCatalog(join(target,"opencodex-catalog.json"))).toBe(true);
  expect(readFileSync(join(defaultTarget,"config.toml"),"utf8")).toBe(defaultConfig);
  expect(readFileSync(join(other,"config.toml"),"utf8")).toBe(otherConfig);
  expect(readFileSync(join(source,"config.json"),"utf8")).toBe(sharedConfig);
}, 45000);

test("认证兼容只补缺失账号 ID，保留令牌和备份，拒绝冲突或链接", async () => {
  const root = mkdtempSync(join(tmpdir(),"opencodex-auth-repair-"));
  temporaryRoots.push(root);
  const engine = resolveOpenCodexPackageRoot(originalOpenCodexPackageRoot);
  const token = `e30.${Buffer.from(JSON.stringify({"https://api.openai.com/auth":{chatgpt_account_id:"test-id"}})).toString("base64url")}.fixture`;
  for(const kind of ["missing", "empty", "valid", "conflict", "malformed", "apikey", "link", "absent"]) {
    const home=join(root,kind);mkdirSync(home);
    const path=join(home,"auth.json");
    const auth:any={tokens:{access_token:token,id_token:token,refresh_token:"unchanged"},last_refresh:"unchanged"};
    if(kind === "empty") auth.tokens.account_id="";
    if(kind === "valid") auth.tokens.account_id="test-id";
    if(kind === "conflict") auth.tokens.account_id="different";
    if(kind === "apikey") auth.auth_mode="apikey";
    const content=kind === "malformed" ? "{" : JSON.stringify(auth);
    if(kind === "link") {writeFileSync(join(root,"external.json"),content);symlinkSync(join(root,"external.json"),path);}
    else if(kind !== "absent") writeFileSync(path,content);
    if(["conflict","malformed","link"].includes(kind)) {
      await expect(repairNativeAuthAccountId(home,engine)).rejects.toThrow();
      expect(readFileSync(path,"utf8")).toBe(content);
    } else {
      const changed=kind === "missing" || kind === "empty";
      expect(await repairNativeAuthAccountId(home,engine)).toBe(changed);
      if(changed) {
        expect(JSON.parse(readFileSync(path,"utf8"))).toEqual({...auth,tokens:{...auth.tokens,account_id:"test-id"}});
        const backups=readdirSync(home).filter(name=>name.endsWith(".bak"));
        expect(backups.length).toBe(1);
        expect(readFileSync(join(home,backups[0]),"utf8")).toBe(content);
        if(process.platform !== "win32") expect(statSync(join(home,backups[0])).mode & 0o777).toBe(0o600);
        expect(await repairNativeAuthAccountId(home,engine)).toBe(false);
      } else if(kind !== "absent") expect(readFileSync(path,"utf8")).toBe(content);
    }
  }
});

afterEach(() => {
  if (originalDefaultInstance === undefined) delete process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE;
  else process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE = originalDefaultInstance;
  if (originalOpenCodexHome === undefined) delete process.env.OPENCODEX_HOME;
  else process.env.OPENCODEX_HOME = originalOpenCodexHome;
  if (originalCodexHome === undefined) delete process.env.CODEX_HOME;
  else process.env.CODEX_HOME = originalCodexHome;
  if (originalOpenCodexPackageRoot === undefined) delete process.env.OPENCODEX_PACKAGE_ROOT;
  else process.env.OPENCODEX_PACKAGE_ROOT = originalOpenCodexPackageRoot;
  for (const root of temporaryRoots.splice(0)) rmSync(root, { recursive: true, force: true });
});

test("目录必须包含有效模型，空目录和损坏目录不作为同步成功", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-empty-source-"));
  const target = mkdtempSync(join(tmpdir(), "opencodex-empty-target-"));
  const pkg = mkdtempSync(join(tmpdir(), "opencodex-empty-package-"));
  temporaryRoots.push(source, target, pkg);
  writeFileSync(join(source, "config.json"), '{"clientIntegrations":{"codex":true}}');
  writeActiveEnginePackage(pkg);
  writeFileSync(join(pkg, "src/codex/refresh.ts"), 'import {writeFileSync} from "node:fs"; import {join} from "node:path"; export async function refreshCodexModelCatalog() {const path=join(process.env.CODEX_HOME!,"empty.json"); writeFileSync(path,\'{"models":[]}\'); return {catalogExists:true,path};}');
  process.env.OPENCODEX_HOME = source;
  process.env.CODEX_HOME = target;
  process.env.OPENCODEX_PACKAGE_ROOT = pkg;
  process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE = "0";
  expect((await syncInstance(15800)).success).toBe(false);
  expect(existsSync(join(target, "active-engine-marker"))).toBe(false);
  const path = join(target, "catalog.json");
  for (const content of ['{"models":[]}', '{', '{"models":[{}]}', '{"models":[{"slug":""}]}']) {
    writeFileSync(path, content); expect(isUsableModelCatalog(path)).toBe(false);
  }
  writeFileSync(path, '{"models":[{"slug":"test/model"}]}');
  expect(isUsableModelCatalog(path)).toBe(true);
});

test("冷启动仅初始化所选实例的空目录，保留备份且拒绝链接和已有模型", () => {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "opencodex-bootstrap-")));
  temporaryRoots.push(root);
  const pkg = join(root, "engine");
  mkdirSync(join(pkg, "src/codex/data"), {recursive:true});
  const template = '{"models":[{"slug":"native-template"}]}';
  writeFileSync(join(pkg, "src/codex/data/upstream-models.json"), template);
  for (const kind of ["missing", "empty", "valid", "invalid", "link", "dangling", "custom"]) {
    const home = join(root, kind);
    mkdirSync(home);
    process.env.CODEX_HOME = home;
    const path = join(home, kind === "custom" ? "custom.json" : "opencodex-catalog.json");
    const external = join(root, `outside-${kind}.json`);
    if (kind === "link" || kind === "dangling") {
      if (kind === "link") writeFileSync(external, '{"models":[]}');
      symlinkSync(external, path);
    } else if (kind !== "missing") {
      writeFileSync(path, kind === "valid" ? template : kind === "invalid" ? "{" : '{"models":[]}');
    }
    const before = existsSync(path) ? readFileSync(path, "utf8") : null;
    const expected = kind === "missing" || kind === "empty";
    expect(seedEmptyCatalogFromEngine(pkg, path)).toBe(expected);
    if (expected) expect(readFileSync(path, "utf8")).toBe(template);
    else if (before !== null) expect(readFileSync(path, "utf8")).toBe(before);
    else expect(existsSync(external)).toBe(false);
    const backups = readdirSync(home).filter(name => name.includes("before-bootstrap"));
    expect(backups.length).toBe(kind === "empty" ? 1 : 0);
    if (kind === "empty") expect(readFileSync(join(home, backups[0]), "utf8")).toBe('{"models":[]}');
  }
});

test("重置前关闭所选实例接入不解析损坏 TOML，也不关闭全局集成", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-disable-source-"));
  const target = mkdtempSync(join(tmpdir(), "opencodex-disable-target-"));
  temporaryRoots.push(source, target);
  const original = '{"clientIntegrations":{"codex":true}}';
  writeFileSync(join(source, "config.json"), original);
  writeFileSync(join(target, "config.toml"), '[broken');
  process.env.OPENCODEX_HOME = source;
  process.env.CODEX_HOME = target;
  process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE = "0";
  expect((await disableInstance()).success).toBe(true);
  expect(JSON.parse(readFileSync(join(target, ".switcher-opencodex/config.json"), "utf8")).clientIntegrations.codex).toBe(false);
  expect(readFileSync(join(source, "config.json"), "utf8")).toBe(original);
  expect(readFileSync(join(target, "config.toml"), "utf8")).toBe('[broken');
});

test("未下载 Engine 时仍可停用旧实例接入以重置配置，不启用回退", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-reset-without-engine-"));
  const target = mkdtempSync(join(tmpdir(), "opencodex-reset-home-"));
  temporaryRoots.push(source, target);
  const original = '{"clientIntegrations":{"codex":true,"other":true},"port":15800}';
  writeFileSync(join(source, "config.json"), original);
  writeFileSync(join(target, "config.toml"), "[broken");
  process.env.OPENCODEX_HOME = source;
  process.env.CODEX_HOME = target;
  delete process.env.OPENCODEX_PACKAGE_ROOT;
  process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE = "0";
  expect((await disableInstance()).success).toBe(true);
  expect(readFileSync(join(source, "config.json"), "utf8")).toBe(original);
  process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE = "1";
  expect((await disableInstance()).success).toBe(true);
  expect(JSON.parse(readFileSync(join(source, "config.json"), "utf8"))).toEqual({
    clientIntegrations:{codex:false,other:true},port:15800,
  });
  expect(readFileSync(join(target, "config.toml"), "utf8")).toBe("[broken");
});

test("Engine 未提交更新时不把旧目录当成功，冲突可有界重试且关闭意图优先", async () => {
  for (const mode of ["unchanged", "retry", "disabled", "converged"]) {
    const source = mkdtempSync(join(tmpdir(), "opencodex-retry-source-"));
    const target = mkdtempSync(join(tmpdir(), "opencodex-retry-target-"));
    const pkg = mkdtempSync(join(tmpdir(), "opencodex-retry-package-"));
    temporaryRoots.push(source, target, pkg);
    writeFileSync(join(source, "config.json"), '{"clientIntegrations":{"codex":true}}');
    writeActiveEnginePackage(pkg);
    writeFileSync(join(pkg, "src/codex/refresh.ts"), `
      import {writeFileSync} from "node:fs";
      import {join} from "node:path";
      let attempts = 0;
      export async function refreshCodexModelCatalog() {
        attempts++;
        const path = join(process.env.CODEX_HOME, "opencodex-catalog.json");
        writeFileSync(path, JSON.stringify({models:[{slug:"old/model"}]}));
        writeFileSync(join(process.env.CODEX_HOME, "attempts"), String(attempts));
        return {path,catalogExists:true,added:${JSON.stringify(mode)} === "converged" ? 1 : 0,catalogWritten:${JSON.stringify(mode)} === "retry" && attempts === 3,
          skippedReason:${JSON.stringify(mode)} === "disabled" ? "desired_disabled" : undefined};
      }
    `);
    process.env.OPENCODEX_HOME = source;
    process.env.CODEX_HOME = target;
    process.env.OPENCODEX_PACKAGE_ROOT = pkg;
    process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE = "0";
    const result = await syncInstance(15800);
    expect(result.success).toBe(mode === "retry" || mode === "converged");
    expect(existsSync(join(target, "active-engine-marker"))).toBe(mode === "retry" || mode === "converged");
    expect(readFileSync(join(target, "attempts"), "utf8")).toBe(mode === "disabled" || mode === "converged" ? "1" : "3");
  }
});

test("只读预检拒绝分页迁移时保留配置、账号和接入状态", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-preflight-"));
  const pkg = mkdtempSync(join(tmpdir(), "opencodex-preflight-package-"));
  temporaryRoots.push(source, pkg);
  writeActiveEnginePackage(pkg);
  writeFileSync(join(source, "config.json"), '{"clientIntegrations":{"codex":false}}');
  writeFileSync(join(source, "auth.json"), "unchanged");
  writeFileSync(join(pkg, "src/codex/inject.ts"),
    'export async function injectCodexConfig(port, config, options) { if(config.syncResumeHistory !== false || !options.validateOnly) throw new Error("unsafe preflight"); return {success:false,message:"history_paginated_requires_native_writer"}; }');
  process.env.OPENCODEX_HOME = source;
  process.env.CODEX_HOME = source;
  process.env.OPENCODEX_PACKAGE_ROOT = pkg;
  process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE = "1";
  expect((await preflightInstance(15800)).message).toContain("分页历史");
  expect((await syncInstance(15800)).success).toBe(false);
  expect(readFileSync(join(source, "config.json"), "utf8")).toBe('{"clientIntegrations":{"codex":false}}');
  expect(readFileSync(join(source, "auth.json"), "utf8")).toBe("unchanged");
  expect(existsSync(join(source, "active-engine-marker"))).toBe(false);
});

test("旧 Engine 无独立历史预检接口时仍能恢复", async () => {
  const home = mkdtempSync(join(tmpdir(), "opencodex-old-restore-"));
  const pkg = mkdtempSync(join(tmpdir(), "opencodex-old-package-"));
  temporaryRoots.push(home, pkg);
  writeActiveEnginePackage(pkg);
  writeFileSync(join(pkg, "src/codex/history-provider.ts"), "export {};");
  process.env.CODEX_HOME = home;
  process.env.OPENCODEX_HOME = home;
  process.env.OPENCODEX_PACKAGE_ROOT = pkg;
  process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE = "1";
  expect((await restoreInstance()).success).toBe(true);
});

test("恢复被分页保护拒绝时不关闭接入或删除配置", async () => {
  const home = mkdtempSync(join(tmpdir(), "opencodex-restore-refusal-"));
  const pkg = mkdtempSync(join(tmpdir(), "opencodex-restore-package-"));
  temporaryRoots.push(home, pkg);
  writeActiveEnginePackage(pkg);
  writeFileSync(join(pkg, "src/codex/history-provider.ts"), 'export function preflightCodexHistoryInjection(){return "history_paginated_requires_native_writer";}');
  writeFileSync(join(pkg, "src/codex/desired-state.ts"), 'export function setCodexIntegrationEnabled(){throw new Error("must not change intent");}');
  writeFileSync(join(home, "config.json"), '{"clientIntegrations":{"codex":true}}');
  process.env.CODEX_HOME = home;
  process.env.OPENCODEX_HOME = home;
  process.env.OPENCODEX_PACKAGE_ROOT = pkg;
  process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE = "1";
  const result = await restoreInstance();
  expect(result.success).toBe(false);
  expect(result.message).toContain("未删除 Engine");
  expect(readFileSync(join(home, "config.json"), "utf8")).toBe('{"clientIntegrations":{"codex":true}}');
});

function writeActiveEnginePackage(root: string): void {
  const codexDir = join(root, "src", "codex");
  mkdirSync(codexDir, { recursive: true });
  writeFileSync(join(codexDir, "history-provider.ts"), 'export function preflightCodexHistoryInjection(){return null;}');
  writeFileSync(join(codexDir, "paths.ts"), 'import {join} from "node:path"; export function resolveCodexStateDbPath(){return join(process.env.CODEX_HOME!, "state_5.sqlite");}');
  writeFileSync(join(root, "src", "config.ts"), [
    "export function loadConfig() { return {}; }",
    "export function applyProxyEnv() {}",
    "export function saveConfig() {}",
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
    'import { writeFileSync } from "node:fs";',
    'import { join } from "node:path";',
    'export async function refreshCodexModelCatalog() { const path = join(process.env.CODEX_HOME!, "opencodex-catalog.json"); writeFileSync(path, JSON.stringify({models:[{slug:"test/model"}]})); return { catalogExists: true, path }; }',
    "",
  ].join("\n"));
  writeFileSync(join(codexDir, "desired-state.ts"), [
    "export function setCodexIntegrationEnabled() { return { ok: true }; }",
    "",
  ].join("\n"));
}

test("多开同步只加载所选下载版本，未安装或缺失时禁止回退", async () => {
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
  expect(() => resolveOpenCodexPackageRoot(join(activePackage, "missing"))).toThrow("重新下载");
  expect(() => resolveOpenCodexPackageRoot("")).toThrow("下载并激活");
  expect(() => resolveOpenCodexPackageRoot("relative/path")).toThrow("下载并激活");
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

test("模型目录刷新失败不会静默返回同步成功或覆盖路由", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-catalog-failure-source-"));
  const target = mkdtempSync(join(tmpdir(), "opencodex-catalog-failure-target-"));
  const activePackage = mkdtempSync(join(tmpdir(), "opencodex-catalog-failure-package-"));
  temporaryRoots.push(source, target, activePackage);
  writeFileSync(join(source, "config.json"), JSON.stringify({clientIntegrations:{codex:true}}));
  writeActiveEnginePackage(activePackage);
  writeFileSync(join(activePackage, "src/codex/refresh.ts"), 'export async function refreshCodexModelCatalog() { throw new Error("catalog unavailable"); }');
  process.env.OPENCODEX_HOME = source;
  process.env.CODEX_HOME = target;
  process.env.OPENCODEX_PACKAGE_ROOT = activePackage;
  const result = await syncInstance(15800);
  expect(result.success).toBe(false);
  expect(result.message).toContain("catalog unavailable");
  expect(existsSync(join(target, "active-engine-marker"))).toBe(false);
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

test("多开实例连续同步和恢复保留默认集成、其他实例和持久恢复记录", async () => {
  const source = mkdtempSync(join(tmpdir(), "opencodex-instance-isolate-source-"));
  const target = mkdtempSync(join(tmpdir(), "opencodex-instance-isolate-target-"));
  const other = mkdtempSync(join(tmpdir(), "opencodex-other-instance-"));
  const activePackage = mkdtempSync(join(tmpdir(), "opencodex-isolation-package-"));
  temporaryRoots.push(source, target, other, activePackage);
  writeActiveEnginePackage(activePackage);
  const globalConfigPath = join(source, "config.json");
  writeFileSync(globalConfigPath, JSON.stringify({
    port: 15800,
    clientIntegrations: { grok: true, codex: true },
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
  const originalConfig = readFileSync(globalConfigPath, "utf8");
  const originalTarget = readFileSync(join(target, "config.toml"), "utf8");
  const run = (action: string, home: string) => {
    const execution = spawnSync(process.execPath,
      [join(import.meta.dir, "manager-instance-integration.ts"), action, "15800"],
      { encoding: "utf8", env: { ...process.env, OPENCODEX_HOME: source,
        CODEX_HOME: home, OPENCODEX_PACKAGE_ROOT: activePackage, OPENCODEX_MANAGER_DEFAULT_INSTANCE: "0" } });
    expect(execution.stderr).toBe("");
    expect(execution.status).toBe(0);
    expect(JSON.parse(execution.stdout).success).toBe(true);
  };
  run("sync", target);
  const overlay = join(target, ".switcher-opencodex");
  writeFileSync(join(overlay, "codex-history-backup-test.json"), "restore evidence");
  run("sync", other);
  run("restore", other);
  run("sync", target);
  expect(readFileSync(join(overlay, "codex-history-backup-test.json"), "utf8")).toBe("restore evidence");
  expect(readFileSync(join(target, "config.toml"), "utf8")).toBe(originalTarget);
  expect(readFileSync(globalConfigPath, "utf8")).toBe(originalConfig);
  expect(JSON.parse(readFileSync(globalConfigPath, "utf8")).clientIntegrations).toEqual({
    grok: true,
    codex: true,
  });
});
