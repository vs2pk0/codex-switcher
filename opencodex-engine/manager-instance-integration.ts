import {
  chmodSync,
  copyFileSync,
  existsSync,
  lstatSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const MAX_CONFIG_FILE_BYTES = 32 * 1024 * 1024;
const OPENCODEX_PACKAGE_ROOT_ENV = "OPENCODEX_PACKAGE_ROOT";
const BUNDLED_OPENCODEX_PACKAGE_ROOT = resolve(
  import.meta.dir,
  "node_modules",
  "@bitkyc08",
  "opencodex",
);
const REQUIRED_OPENCODEX_MODULES = [
  ["src", "config.ts"],
  ["src", "codex", "desired-state.ts"],
  ["src", "codex", "inject.ts"],
  ["src", "codex", "refresh.ts"],
] as const;
const OVERLAY_FILES = [
  "config.json",
  "auth.json",
  "codex-accounts.json",
  "codex-quota-cache.json",
  "thought-signature-replay.salt",
] as const;

type InstanceIntegrationAction = "isolate-default" | "sync" | "restore";

interface InstanceIntegrationResult {
  action: InstanceIntegrationAction;
  success: boolean;
  message: string;
}

function sourceConfigDir(): string {
  const configured = process.env.OPENCODEX_HOME?.trim();
  return resolve(configured || join(homedir(), ".opencodex"));
}

function isOpenCodexPackageRoot(path: string): boolean {
  return REQUIRED_OPENCODEX_MODULES.every((segments) => existsSync(join(path, ...segments)));
}

/**
 * The manager helper ships with the application, while the service may run a
 * newer managed OpenCodex package. Prefer the package root passed by the
 * backend and retain the bundled package as a compatibility fallback.
 */
export function resolveOpenCodexPackageRoot(
  configuredRoot = process.env[OPENCODEX_PACKAGE_ROOT_ENV],
): string {
  const requested = configuredRoot?.trim();
  if (requested) {
    const activeRoot = resolve(requested);
    if (isOpenCodexPackageRoot(activeRoot)) return activeRoot;
  }
  if (isOpenCodexPackageRoot(BUNDLED_OPENCODEX_PACKAGE_ROOT)) {
    return BUNDLED_OPENCODEX_PACKAGE_ROOT;
  }
  throw new Error("客户端内置 OpenCodex Engine 缺失，请重新安装完整客户端");
}

function openCodexModuleUrl(packageRoot: string, segments: readonly string[]): string {
  return pathToFileURL(join(packageRoot, ...segments)).href;
}

async function importOpenCodexModule(packageRoot: string, segments: readonly string[]) {
  return import(openCodexModuleUrl(packageRoot, segments));
}

function copyRegularConfigFile(sourceDir: string, overlayDir: string, name: string): void {
  const source = join(sourceDir, name);
  if (!existsSync(source)) return;
  const metadata = lstatSync(source);
  if (!metadata.isFile() || metadata.size > MAX_CONFIG_FILE_BYTES) {
    throw new Error(`OpenCodex 配置文件不安全或过大：${name}`);
  }
  const target = join(overlayDir, name);
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
): Promise<T> {
  const overlayDir = mkdtempSync(join(tmpdir(), "codex-switcher-opencodex-"));
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
    rmSync(overlayDir, { recursive: true, force: true });
  }
}

export async function syncInstance(port: number): Promise<InstanceIntegrationResult> {
  return withIsolatedOpenCodexConfig(sourceConfigDir(), true, async () => {
    const packageRoot = resolveOpenCodexPackageRoot();
    const [{ applyProxyEnv, loadConfig }, { injectCodexConfig }, { refreshCodexModelCatalog }] =
      await Promise.all([
        importOpenCodexModule(packageRoot, ["src", "config.ts"]),
        importOpenCodexModule(packageRoot, ["src", "codex", "inject.ts"]),
        importOpenCodexModule(packageRoot, ["src", "codex", "refresh.ts"]),
      ]);
    const config = loadConfig();
    const preflight = await injectCodexConfig(port, config, { validateOnly: true });
    if (!preflight.success) return { action: "sync", success: false, message: preflight.message };

    applyProxyEnv(config);
    let catalogPath: string | null = null;
    try {
      const catalog = await refreshCodexModelCatalog(config);
      catalogPath = catalog.catalogExists ? catalog.path : null;
    } catch {
      // The routing repair remains useful when a provider catalog is temporarily unavailable.
    }
    const injected = await injectCodexConfig(port, config, { catalogPath });
    return { action: "sync", success: injected.success, message: injected.message };
  });
}

export async function restoreInstance(): Promise<InstanceIntegrationResult> {
  return withIsolatedOpenCodexConfig(sourceConfigDir(), false, async () => {
    const { restoreNativeCodexAsync } = await importOpenCodexModule(
      resolveOpenCodexPackageRoot(),
      ["src", "codex", "inject.ts"],
    );
    const restored = await restoreNativeCodexAsync();
    return { action: "restore", success: restored.success, message: restored.message };
  });
}

export async function isolateDefaultInstance(): Promise<InstanceIntegrationResult> {
  const packageRoot = resolveOpenCodexPackageRoot();
  const { setCodexIntegrationEnabled } = await importOpenCodexModule(
    packageRoot,
    ["src", "codex", "desired-state.ts"],
  );
  const desired = setCodexIntegrationEnabled(false);
  if (!desired.ok) {
    return {
      action: "isolate-default",
      success: false,
      message: `无法保存系统默认实例的 OpenCodex 关闭状态（${desired.reason}）`,
    };
  }

  const { restoreNativeCodexAsync } = await importOpenCodexModule(
    packageRoot,
    ["src", "codex", "inject.ts"],
  );
  const restored = await restoreNativeCodexAsync({ revalidateDesiredState: true });
  const routingRestored = restored.artifacts.config.state !== "failed"
    && restored.artifacts.catalog.state !== "failed";
  if (!routingRestored) {
    return {
      action: "isolate-default",
      success: false,
      message: `系统默认实例未能恢复原生配置：${restored.message}`,
    };
  }

  const historyWarning = restored.artifacts.history.state === "failed"
    ? `；会话历史暂未恢复：${restored.artifacts.history.message}`
    : "";
  return {
    action: "isolate-default",
    success: true,
    message: `系统默认实例已与 OpenCodex 隔离${historyWarning}`,
  };
}

function parseAction(value: string | undefined): InstanceIntegrationAction {
  if (value === "isolate-default" || value === "sync" || value === "restore") return value;
  throw new Error("多开实例集成操作只支持 isolate-default、sync 或 restore");
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
  const result = action === "isolate-default"
    ? await isolateDefaultInstance()
    : action === "sync"
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
