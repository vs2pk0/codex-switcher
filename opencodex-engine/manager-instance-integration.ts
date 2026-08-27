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

const MAX_CONFIG_FILE_BYTES = 32 * 1024 * 1024;
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
    const [{ applyProxyEnv, loadConfig }, { injectCodexConfig }, { refreshCodexModelCatalog }] =
      await Promise.all([
        import("./node_modules/@bitkyc08/opencodex/src/config.ts"),
        import("./node_modules/@bitkyc08/opencodex/src/codex/inject.ts"),
        import("./node_modules/@bitkyc08/opencodex/src/codex/refresh.ts"),
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
    const { restoreNativeCodexAsync } = await import(
      "./node_modules/@bitkyc08/opencodex/src/codex/inject.ts"
    );
    const restored = await restoreNativeCodexAsync();
    return { action: "restore", success: restored.success, message: restored.message };
  });
}

export async function isolateDefaultInstance(): Promise<InstanceIntegrationResult> {
  const { setCodexIntegrationEnabled } = await import(
    "./node_modules/@bitkyc08/opencodex/src/codex/desired-state.ts"
  );
  const desired = setCodexIntegrationEnabled(false);
  if (!desired.ok) {
    return {
      action: "isolate-default",
      success: false,
      message: `无法保存系统默认实例的 OpenCodex 关闭状态（${desired.reason}）`,
    };
  }

  const { restoreNativeCodexAsync } = await import(
    "./node_modules/@bitkyc08/opencodex/src/codex/inject.ts"
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
