import {
  chmodSync,
  closeSync,
  constants,
  copyFileSync,
  existsSync,
  lstatSync,
  fstatSync,
  fsyncSync,
  ftruncateSync,
  mkdtempSync,
  mkdirSync,
  openSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
  writeFileSync,
  writeSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { randomUUID } from "node:crypto";
import { resolveOpenCodexPackageRoot } from "./manager-engine-package.ts";
export { resolveOpenCodexPackageRoot } from "./manager-engine-package.ts";

const MAX_CONFIG_FILE_BYTES = 32 * 1024 * 1024;
const OVERLAY_FILES = [
  "config.json",
  "auth.json",
  "codex-accounts.json",
  "codex-quota-cache.json",
  "thought-signature-replay.salt",
  "codex-runtime.json",
] as const;

type InstanceIntegrationAction = "sync" | "restore" | "disable";

interface InstanceIntegrationResult {
  action: InstanceIntegrationAction;
  success: boolean;
  message: string;
}

function sourceConfigDir(): string {
  const configured = process.env.OPENCODEX_MANAGER_SOURCE_HOME?.trim()
    || process.env.OPENCODEX_HOME?.trim();
  return resolve(configured || join(homedir(), ".opencodex"));
}

function openCodexModuleUrl(packageRoot: string, segments: readonly string[]): string {
  return pathToFileURL(join(packageRoot, ...segments)).href;
}

async function importOpenCodexModule(packageRoot: string, segments: readonly string[]) {
  return import(openCodexModuleUrl(packageRoot, segments));
}

function copyRegularConfigFile(sourceDir: string, overlayDir: string, name: string): void {
  const source = join(sourceDir, name);
  const target = join(overlayDir, name);
  if (existsSync(target) && (!lstatSync(target).isFile() || lstatSync(target).isSymbolicLink())) {
    throw new Error(`实例配置目标必须是普通文件：${name}`);
  }
  if (!existsSync(source)) {
    if (existsSync(target)) rmSync(target);
    return;
  }
  const metadata = lstatSync(source);
  if (!metadata.isFile() || metadata.size > MAX_CONFIG_FILE_BYTES) {
    throw new Error(`OpenCodex 配置文件不安全或过大：${name}`);
  }
  copyFileSync(source, target);
  chmodSync(target, 0o600);
}

function writeOverlayConfig(overlayDir: string, enabled: boolean): void {
  const path = join(overlayDir, "config.json");
  if (!existsSync(path)) throw new Error("OpenCodex 尚未初始化，缺少 config.json");
  const parsed = JSON.parse(readFileSync(path, "utf8")) as Record<string, unknown>;
  if (!parsed || Array.isArray(parsed) || typeof parsed !== "object") {
    throw new Error("OpenCodex config.json 顶层必须是对象");
  }
  const integrations = parsed.clientIntegrations;
  parsed.clientIntegrations = {
    ...(integrations && !Array.isArray(integrations) && typeof integrations === "object"
      ? integrations as Record<string, unknown>
      : {}),
    codex: enabled,
  };
  writeFileSync(path, `${JSON.stringify(parsed, null, 2)}\n`, { mode: 0o600 });
}

export async function withIsolatedOpenCodexConfig<T>(
  sourceDir: string,
  enabled: boolean,
  operation: (overlayDir: string) => Promise<T>,
  persistentDir?: string,
): Promise<T> {
  const overlayDir = persistentDir ?? mkdtempSync(join(tmpdir(), "codex-switcher-opencodex-"));
  mkdirSync(overlayDir, { recursive: true, mode: 0o700 });
  if (!lstatSync(overlayDir).isDirectory() || lstatSync(overlayDir).isSymbolicLink()) {
    throw new Error("实例集成状态目录必须是普通目录");
  }
  chmodSync(overlayDir, 0o700);
  const previousHome = process.env.OPENCODEX_HOME;
  try {
    mkdirSync(overlayDir, { recursive: true, mode: 0o700 });
    for (const name of OVERLAY_FILES) copyRegularConfigFile(sourceDir, overlayDir, name);
    writeOverlayConfig(overlayDir, enabled);
    process.env.OPENCODEX_HOME = overlayDir;
    return await operation(overlayDir);
  } finally {
    if (previousHome === undefined) delete process.env.OPENCODEX_HOME;
    else process.env.OPENCODEX_HOME = previousHome;
    if (!persistentDir) rmSync(overlayDir, { recursive: true, force: true });
  }
}

async function withInstanceConfig<T>(enabled: boolean, operation: () => Promise<T>): Promise<T> {
  if (process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE === "1") {
    const { setCodexIntegrationEnabled } = await importOpenCodexModule(
      resolveOpenCodexPackageRoot(), ["src", "codex", "desired-state.ts"],
    );
    const result = setCodexIntegrationEnabled(enabled);
    if (!result.ok) throw new Error(`无法保存默认实例集成状态：${result.reason}`);
    return operation();
  }
  const home = process.env.CODEX_HOME?.trim();
  if (!home) throw new Error("实例操作必须指定 CODEX_HOME");
  // Engine provenance, catalog backups and history manifests must survive the
  // process. A fresh temporary identity on every sync loses restore authority.
  return withIsolatedOpenCodexConfig(sourceConfigDir(), enabled, operation,
    join(resolve(home), ".switcher-opencodex"));
}

export async function syncInstance(port: number): Promise<InstanceIntegrationResult> {
  return withInstanceConfig(true, async () => {
    const packageRoot = resolveOpenCodexPackageRoot();
    const home = process.env.CODEX_HOME?.trim();
    if (!home) throw new Error("实例操作必须指定 CODEX_HOME");
    await repairNativeAuthAccountId(home, packageRoot);
    const [{ applyProxyEnv, loadConfig }, { injectCodexConfig }, { refreshCodexModelCatalog }] =
      await Promise.all([
        importOpenCodexModule(packageRoot, ["src", "config.ts"]),
        importOpenCodexModule(packageRoot, ["src", "codex", "inject.ts"]),
        importOpenCodexModule(packageRoot, ["src", "codex", "refresh.ts"]),
      ]);
    let config = loadConfig();
    const preflight = await injectCodexConfig(port, config, { validateOnly: true });
    if (!preflight.success) return { action: "sync", success: false, message: preflight.message };

    applyProxyEnv(config);
    let catalogPath: string | null = null;
    try {
      for (let attempt = 0; attempt < 3; attempt++) {
        const catalog = await refreshCodexModelCatalog(config);
        if (catalog.skippedReason === "desired_disabled") {
          return { action: "sync", success: false, message: "所选实例的 OpenCodex 接入已关闭，模型目录未更新；已停止同步。" };
        }
        // Engine reports added > 0 after a serialized, validated no-op commit.
        // A conflict/absent baseline instead returns added: 0. A merely valid
        // old file is insufficient evidence that discovery has converged.
        if (catalog.catalogWritten === false && !(catalog.added > 0)) {
          if (attempt === 0) {
            const lockModule = ["src", "codex", "catalog-write-serialization.ts"];
            if (existsSync(join(packageRoot, ...lockModule))) {
              const { withCatalogWriteSerialization } = await importOpenCodexModule(packageRoot, lockModule);
              withCatalogWriteSerialization(realpathSync(process.env.CODEX_HOME!),
                () => seedEmptyCatalogFromEngine(packageRoot, catalog.path));
            }
          }
          if (attempt === 2) {
            return { action: "sync", success: false, message: "Engine 连续 3 次未提交模型目录更新（可能存在配置变更或写入冲突）；未使用旧目录冒充同步成功。请查看运行日志后重试。" };
          }
          await new Promise(resolve => setTimeout(resolve, 150 * (attempt + 1)));
          config = loadConfig();
          applyProxyEnv(config);
          continue;
        }
        if (!catalog.catalogExists || !isUsableModelCatalog(catalog.path)) {
          return { action: "sync", success: false, message: "Engine 未生成包含有效模型的目录；实例存在，但模型目录为空、缺失或格式无效。请检查账号和模型发现结果后重新同步。" };
        }
        catalogPath = catalog.path;
        break;
      }
    } catch (error) {
      return { action: "sync", success: false, message: `模型目录刷新失败：${error instanceof Error ? error.message : String(error)}` };
    }
    const injected = await injectCodexConfig(port, config, { catalogPath });
    return { action: "sync", success: injected.success, message: injected.message };
  });
}

/** Engine's snapshot and revalidation must observe the same native identity.
 * Older Switcher projections omitted account_id. Repair only a missing field,
 * preserving credentials and an owner-only backup; never change a conflicting ID.
 */
export async function repairNativeAuthAccountId(home: string, packageRoot: string): Promise<boolean> {
  const path = join(realpathSync(home), "auth.json");
  let fd: number;
  try {
    const stat = lstatSync(path);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1) {
      throw new Error("认证文件必须是独立的普通文件，未自动修复");
    }
    fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return false;
    throw error;
  }
  let content: string;
  let before: ReturnType<typeof fstatSync>;
  try {
    before = fstatSync(fd);
    if (!before.isFile() || before.nlink !== 1 || before.size > MAX_CONFIG_FILE_BYTES) {
      throw new Error("认证文件不安全或过大，未自动修复");
    }
    content = readFileSync(fd, "utf8");
  } finally { closeSync(fd); }
  let auth;
  try { auth = JSON.parse(content); }
  catch { throw new Error("auth.json 格式无效，请修复认证文件后重新同步"); }
  if (auth?.auth_mode === "apikey" || !auth?.tokens || typeof auth.tokens !== "object"
      || Array.isArray(auth.tokens) || typeof auth.tokens.access_token !== "string") return false;
  const { extractAccountId } = await importOpenCodexModule(packageRoot, ["src", "oauth", "chatgpt.ts"]);
  const id = extractAccountId(typeof auth.tokens.id_token === "string" ? auth.tokens.id_token : undefined,
    auth.tokens.access_token);
  if (typeof id !== "string" || !id.trim()) return false;
  const stored = auth.tokens.account_id;
  if (stored === id) return false;
  if (stored != null && stored !== "") {
    throw new Error("auth.json 的 account_id 与 Token 账号不一致，未覆盖认证信息；请重新登录或切换正确账号后同步");
  }
  auth.tokens.account_id = id;
  const temporary = `${path}.account-id-${randomUUID()}.tmp`;
  const backup = `${path}.account-id-${randomUUID()}.bak`;
  writeFileSync(backup, content, {mode:0o600,flag:"wx"});
  try {
    const output = openSync(temporary, "wx", 0o600);
    try { writeFileSync(output, `${JSON.stringify(auth, null, 2)}\n`); fsyncSync(output); }
    finally { closeSync(output); }
    const current = lstatSync(path);
    if (!current.isFile() || current.isSymbolicLink() || current.nlink !== 1
        || current.ino !== before.ino || current.dev !== before.dev
        || readFileSync(path, "utf8") !== content) {
      throw new Error("修复期间认证文件已变化，未覆盖，请重试同步");
    }
    renameSync(temporary, path);
    return true;
  } finally {
    if (existsSync(temporary)) rmSync(temporary);
  }
}

/** A cold home needs a native template before Engine discovery can commit.
 * Use only the downloaded Engine's own snapshot, never invented model rows.
 * The caller must still obtain a successful refresh before routing is injected.
 */
export function seedEmptyCatalogFromEngine(packageRoot: string, catalogPath: string): boolean {
  const home = process.env.CODEX_HOME?.trim();
  if (!home || resolve(catalogPath) !== join(realpathSync(home), "opencodex-catalog.json")) return false;
  const template = join(packageRoot, "src", "codex", "data", "upstream-models.json");
  if (!isUsableModelCatalog(template)) return false;
  let fd: number | undefined;
  try {
    let present = false;
    try {
      const stat = lstatSync(catalogPath);
      if (!stat.isFile() || stat.isSymbolicLink()) return false;
      present = true;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") return false;
    }
    // Exclusive creation rejects dangling links; NOFOLLOW prevents a replaced
    // existing path from redirecting writes outside the selected instance.
    fd = openSync(catalogPath, present
      ? constants.O_RDWR | constants.O_NOFOLLOW
      : constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL, 0o600);
    const stat = fstatSync(fd);
    if (!stat.isFile() || stat.nlink !== 1 || stat.size > MAX_CONFIG_FILE_BYTES) return false;
    if (present) {
      const content = readFileSync(fd, "utf8");
      const previous = JSON.parse(content);
      if (!Array.isArray(previous?.models) || previous.models.length !== 0) return false;
      writeFileSync(`${catalogPath}.before-bootstrap-${randomUUID()}.bak`, content,
        {mode:0o600, flag:"wx"});
    }
    const content = readFileSync(template);
    // readFileSync advances the descriptor, so write at position zero.
    ftruncateSync(fd, 0);
    let offset = 0;
    while (offset < content.length) {
      const written = writeSync(fd, content, offset, content.length - offset, offset);
      if (written === 0) throw new Error("模型目录写入未完成");
      offset += written;
    }
    return true;
  } catch {
    return false;
  } finally {
    if (fd !== undefined) closeSync(fd);
  }
}

export function isUsableModelCatalog(path: string): boolean {
  try {
    const stat = lstatSync(path);
    if (!stat.isFile() || stat.size > MAX_CONFIG_FILE_BYTES) return false;
    const catalog = JSON.parse(readFileSync(path, "utf8"));
    return Array.isArray(catalog?.models) && catalog.models.length > 0
      && catalog.models.every((model: unknown) => model && typeof model === "object"
        && typeof (model as {slug?: unknown}).slug === "string"
        && (model as {slug: string}).slug.trim().length > 0);
  } catch { return false; }
}

export async function disableInstance(): Promise<InstanceIntegrationResult> {
  // No native TOML parsing: reset must work even when the old TOML is broken.
  const home = process.env.CODEX_HOME?.trim();
  if (!home) throw new Error("实例操作必须指定 CODEX_HOME");
  const directory = process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE === "1"
    ? sourceConfigDir() : join(resolve(home), ".switcher-opencodex");
  if (!existsSync(join(sourceConfigDir(), "config.json"))) {
    if (existsSync(join(directory, "config.json"))) writeOverlayConfig(directory, false);
    return { action: "disable", success: true, message: "已关闭所选实例集成" };
  }
  if (process.env.OPENCODEX_MANAGER_DEFAULT_INSTANCE === "1"
      && !process.env.OPENCODEX_PACKAGE_ROOT?.trim()) {
    // Local reset remains usable when the old bundled Engine has been removed
    // and no downloaded version is active. Do not import any Engine fallback.
    writeOverlayConfig(directory, false);
    return { action: "disable", success: true, message: "已关闭所选实例集成" };
  }
  return withInstanceConfig(false, async () => ({ action: "disable", success: true, message: "已关闭所选实例集成" }));
}

export async function restoreInstance(): Promise<InstanceIntegrationResult> {
  return withInstanceConfig(false, async () => {
    const { restoreNativeCodexAsync } = await importOpenCodexModule(
      resolveOpenCodexPackageRoot(),
      ["src", "codex", "inject.ts"],
    );
    const restored = await restoreNativeCodexAsync();
    return { action: "restore", success: restored.success, message: restored.message };
  });
}

function parseAction(value: string | undefined): InstanceIntegrationAction {
  if (value === "sync" || value === "restore" || value === "disable") return value;
  throw new Error("实例集成操作只支持 sync 或 restore");
}

function parsePort(value: string | undefined): number {
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1024 || port > 65535) {
    throw new Error("端口必须在 1024–65535 之间");
  }
  return port;
}

async function main(): Promise<void> {
  const action = parseAction(process.argv[2]);
  const result = action === "disable" ? await disableInstance() : action === "sync"
      ? await syncInstance(parsePort(process.argv[3]))
      : await restoreInstance();
  process.stdout.write(JSON.stringify(result));
  if (!result.success) process.exitCode = 1;
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : "多开实例 OpenCodex 操作失败");
    process.exitCode = 1;
  });
}
