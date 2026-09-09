import { createHash, randomUUID } from "node:crypto";
import { chmodSync, closeSync, copyFileSync, existsSync, fsyncSync, lstatSync, mkdirSync, openSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, resolve, sep } from "node:path";

type ObjectMap = Record<string, any>;
export interface TransferInput {
  source: string; target: string; manager: string; port: number;
  mode: "merge" | "overwrite"; history: boolean; execute: boolean; fingerprint?: string;
  sourceEngine?: string; targetEngine?: string | null;
}
const HISTORY_FILES = ["usage.jsonl"] as const;
const LOCAL_KEYS = ["port", "hostname", "clientIntegrations", "configRebaseProvenance",
  "codexAutoStart", "codexShimAutoRestore", "codexDesktopAuthless", "apiToken", "adminToken"];

function safePath(path: string): string {
  if (!isAbsolute(path)) throw new Error("传输目录必须为绝对路径");
  let current = resolve(path);
  while (true) {
    if (existsSync(current)) {
      const stat = lstatSync(current);
      if (stat.isSymbolicLink() || !stat.isDirectory()) throw new Error("传输目录不能经过符号链接");
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
  return resolve(path);
}
function objectFile(path: string): ObjectMap {
  if (!existsSync(path)) return {};
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1 || stat.size > 32 * 1024 * 1024)
    throw new Error("配置必须是小于 32 MB 的独立普通文件");
  const data = JSON.parse(readFileSync(path, "utf8"));
  if (!data || typeof data !== "object" || Array.isArray(data)) throw new Error("配置顶层必须是对象");
  return data;
}
function writeJson(path: string, data: unknown): void {
  const temporary = path + "." + randomUUID() + ".tmp";
  const descriptor = openSync(temporary,"wx",0o600);
  try {
    writeFileSync(descriptor, JSON.stringify(data, null, 2) + "\n");
    fsyncSync(descriptor);
  } finally { closeSync(descriptor); }
  try { renameSync(temporary,path); }
  finally { if (existsSync(temporary)) rmSync(temporary); }
}
function copyTree(source: string, target: string): void {
  const stat = lstatSync(source);
  if (stat.isSymbolicLink() || (!stat.isDirectory() && !stat.isFile()) || (stat.isFile() && stat.nlink !== 1))
    throw new Error("数据目录包含链接或特殊文件，已中止传输");
  if (stat.isDirectory()) {
    mkdirSync(target, { recursive: true, mode: 0o700 });
    for (const name of readdirSync(source)) copyTree(join(source, name), join(target, name));
  } else {
    copyFileSync(source, target);
    chmodSync(target, 0o600);
  }
}
export function mergeConfig(source: ObjectMap, target: ObjectMap, mode: "merge" | "overwrite", port: number): ObjectMap {
  const result = mode === "overwrite" ? { ...source } : { ...source, ...target };
  if (mode === "merge") {
    result.providers = { ...source.providers, ...target.providers };
    const targetAccounts = target.codexAccounts ?? [];
    const ids = new Set(targetAccounts.map((a: ObjectMap) => a.id));
    result.codexAccounts = [...targetAccounts, ...(source.codexAccounts ?? []).filter((a: ObjectMap) => !ids.has(a.id))];
    result.codexAccountNamespaces = { ...source.codexAccountNamespaces, ...target.codexAccountNamespaces };
  }
  for (const key of LOCAL_KEYS) {
    delete result[key];
    if (Object.hasOwn(target, key)) result[key] = target[key];
  }
  result.port = port;
  result.hostname = target.hostname || "127.0.0.1";
  result.clientIntegrations = target.clientIntegrations ?? {
    ...Object.fromEntries(Object.keys(source.clientIntegrations ?? {}).map(key => [key,false])),
    codex: false, claudeCode: false, claudeDesktop: false, opencode: false, grok: false,
  };
  result.codexAutoStart = target.codexAutoStart ?? false;
  return result;
}
function rewritePaths(value: any, source: string, target: string, key = ""): any {
  if (typeof value === "string") {
    if (value === source || value.startsWith(source + sep)) return target + value.slice(source.length);
    if (/(path|dir|directory|home)$/i.test(key) && isAbsolute(value))
      throw new Error("配置含外部绝对目录，请先将存储配置改为实例目录后再传输");
    return value;
  }
  if (Array.isArray(value)) return value.map(v => rewritePaths(v, source, target, key));
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([k,v]) => [k,rewritePaths(v,source,target,k)]));
  return value;
}

function copyPortableStorage(config: ObjectMap, source: string, stage: string): void {
  const paths = new Set(["artifacts", "responses-state.json", "codex-quota-cache.json"]);
  function collect(value: any, key = "") {
    if (typeof value === "string" && /(path|dir|directory|home)$/i.test(key) && value.startsWith(source + sep)) {
      const relative = resolve(value).slice(source.length + 1);
      if (!resolve(value).startsWith(source + sep)) throw new Error("存储目录越界");
      paths.add(relative);
    } else if (Array.isArray(value)) value.forEach(item => collect(item, key));
    else if (value && typeof value === "object") Object.entries(value).forEach(([name, item]) => collect(item, name));
  }
  collect(config);
  for (const relative of paths) {
    const path = join(source, relative), destination = join(stage, relative);
    if (!existsSync(path) || existsSync(destination)) continue;
    // Configuration must not smuggle source runtime/admin/integration identity into the clone.
    if (/^(?:admin|service|runtime|integrations|codex-shim|ocx\.pid|config\.json|auth\.json|codex-accounts\.json)/.test(relative))
      throw new Error("存储路径引用了实例运行身份文件");
    safePath(dirname(path));
    mkdirSync(dirname(destination), { recursive: true, mode: 0o700 });
    copyTree(path, destination);
  }
}
export function recoverTransfer(manager: string, target: string): void {
  const journal = join(safePath(manager), "transfer-journal.json");
  if (!existsSync(journal)) return;
  const state = objectFile(journal);
  if (state.target !== safePath(target) || dirname(state.backup) !== manager || dirname(state.stage) !== dirname(target))
    throw new Error("数据传输恢复记录无效");
  if (state.phase === "committed") { rmSync(journal); return; }
  if (existsSync(state.backup)) {
    if (existsSync(target)) renameSync(target, join(manager, "transfer-interrupted-" + randomUUID()));
    renameSync(state.backup, target);
  }
  if (existsSync(state.stage)) rmSync(state.stage, { recursive: true });
  rmSync(journal);
}
export function transferData(input: TransferInput) {
  const source = safePath(input.source), target = safePath(input.target), manager = safePath(input.manager);
  if (source === target || source.startsWith(target + sep) || target.startsWith(source + sep))
    throw new Error("源和目标目录不能相同或重叠");
  if (!["merge", "overwrite"].includes(input.mode)) throw new Error("未知传输模式");
  if (!Number.isInteger(input.port) || input.port < 1024 || input.port > 65535) throw new Error("目标端口无效");
  if (!existsSync(join(source, "config.json"))) throw new Error("源实例尚未初始化");
  if (input.history && input.mode !== "overwrite") throw new Error("用量历史只支持覆盖，不支持合并");
  const targetWasInitialized = existsSync(join(target, "config.json"));
  const sourceConfig = objectFile(join(source, "config.json"));
  const targetConfig = objectFile(join(target, "config.json"));
  const sourceCredentials = objectFile(join(source, "codex-accounts.json"));
  const targetCredentials = objectFile(join(target, "codex-accounts.json"));
  const sourceAuth = objectFile(join(source,"auth.json")), targetAuth = objectFile(join(target,"auth.json"));
  const merged = mergeConfig(sourceConfig, targetConfig, input.mode, input.port);
  const rows = [
    { name: "渠道", source: Object.keys(sourceConfig.providers ?? {}).length, target: Object.keys(targetConfig.providers ?? {}).length, result: Object.keys(merged.providers ?? {}).length },
    { name: "账号", source: (sourceConfig.codexAccounts ?? []).length, target: (targetConfig.codexAccounts ?? []).length, result: (merged.codexAccounts ?? []).length },
  ];
  // A conflicting account ID must retain both its destination metadata and its credential.
  const targetIds = new Set((targetConfig.codexAccounts ?? []).map((a: ObjectMap) => a.id));
  const credentials = input.mode === "overwrite" ? sourceCredentials : {
    ...Object.fromEntries(Object.entries(sourceCredentials).filter(([id]) => !targetIds.has(id))), ...targetCredentials,
  };
  const portable = { ...merged };
  for (const key of LOCAL_KEYS) delete portable[key];
  const rewritten = rewritePaths(portable, source, target);
  const outputConfig = { ...rewritten };
  for (const key of LOCAL_KEYS) if (Object.hasOwn(merged,key)) outputConfig[key] = merged[key];
  const fingerprint = createHash("sha256").update(JSON.stringify([sourceConfig,targetConfig,sourceCredentials,targetCredentials,sourceAuth,targetAuth,input.sourceEngine,input.targetEngine])).digest("hex");
  if (!input.execute) return { rows, fingerprint, backupPath: null, message: "预览完成；同 ID 渠道和账号在合并时保留目标值" };
  if (input.fingerprint !== fingerprint) throw new Error("源或目标数据已变化，请重新预览后再执行");
  mkdirSync(manager, { recursive: true, mode: 0o700 });
  recoverTransfer(manager, target);
  const suffix = randomUUID();
  const stage = join(dirname(target), ".opencodex-transfer-" + suffix);
  const backup = join(manager, "transfer-backup-" + suffix);
  const journal = join(manager, "transfer-journal.json");
  try {
    if (existsSync(target)) copyTree(target, stage);
    else mkdirSync(stage, { recursive: true, mode: 0o700 });
    if (input.sourceEngine && !targetWasInitialized) copyPortableStorage(sourceConfig, source, stage);
    writeJson(join(stage,"config.json"), outputConfig);
    writeJson(join(stage,"codex-accounts.json"), credentials);
    writeJson(join(stage,"auth.json"), input.mode === "merge" ? { ...sourceAuth, ...targetAuth } : sourceAuth);
    if (input.history) for (const name of HISTORY_FILES) {
      if (existsSync(join(stage,name))) rmSync(join(stage,name));
      if (existsSync(join(source,name))) copyTree(join(source,name), join(stage,name));
    }
    // Existing destination SQLite files (including WAL) remain coherent in the
    // stopped snapshot. Source runtime, admin token and integration identity are never copied.
    writeJson(journal, { target, stage, backup, phase: "prepared" });
    if (existsSync(target)) renameSync(target, backup);
    renameSync(stage,target);
    writeJson(journal, { target, stage, backup, phase: "committed" });
    rmSync(journal);
    return { rows, fingerprint, backupPath: existsSync(backup) ? backup : null, message: "数据传输完成，目标端口和实例身份已保留" };
  } catch (error) {
    if (existsSync(journal)) recoverTransfer(manager,target);
    else if (existsSync(stage)) rmSync(stage,{recursive:true});
    throw error;
  }
}
if (import.meta.main) {
  try {
    const input = JSON.parse(await Bun.stdin.text());
    if (process.argv[2] === "recover") { recoverTransfer(input.manager,input.target); process.stdout.write("{}"); }
    else process.stdout.write(JSON.stringify(transferData(input)));
  } catch (error) { console.error(error instanceof Error ? error.message : "数据传输失败"); process.exitCode = 1; }
}
