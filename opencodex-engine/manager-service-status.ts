import { importEngineModule } from "./manager-engine-package.ts";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
const { diagnoseService } = await importEngineModule("src/service.ts");
const {
  getConfigDir,
  loadConfig,
  saveConfig,
} = await importEngineModule("src/config.ts");

export function readServiceReferences(paths: string[]) {
  const referencedCliPaths: string[] = [];
  let referencesUnknown = false;
  for (const path of new Set(paths)) {
    try {
      const state = JSON.parse(readFileSync(path, "utf8"));
      if (typeof state?.cliPath === "string" && state.cliPath.trim()) referencedCliPaths.push(state.cliPath);
      else referencesUnknown = true;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") referencesUnknown = true;
    }
  }
  if (!referencedCliPaths.length) referencesUnknown = true;
  return { referencedCliPaths: [...new Set(referencedCliPaths)], referencesUnknown };
}

export function readBackgroundServiceState() {
  const diagnostic = diagnoseService();
  const references = diagnostic.installed
    ? readServiceReferences([join(getConfigDir(), "service-state.json")])
    : { referencedCliPaths: [], referencesUnknown: false };
  return {
    supported: diagnostic.supported,
    installed: diagnostic.installed,
    enabled: diagnostic.enabled,
    running: diagnostic.running,
    viable: diagnostic.viable,
    stale: diagnostic.stale,
    conflict: diagnostic.conflict,
    backend: diagnostic.backend,
    summary: diagnostic.summary,
    ...references,
  };
}

export function restoreDisabledAutostart(
  diagnose = diagnoseService,
  run: (command: string, args: string[]) => void = (command, args) => { execFileSync(command, args, { stdio: "pipe", timeout: 15_000 }); },
): void {
  // Repair enables systemd units; preserve a running-but-disabled registration.
  const state = diagnose();
  if (!state.installed || state.conflict || state.backend !== "systemd") throw new Error("后台服务类型已变化，无法恢复自启设置");
  run("systemctl", ["--user", "disable", process.env.OPENCODEX_SYSTEM_SERVICE_NAME || "opencodex-proxy"]);
  if (diagnose().enabled) throw new Error("未能恢复后台服务原有的禁用自启设置");
}

export function setConfiguredServicePort(port: number): { port: number } {
  if (!Number.isInteger(port) || port < 1024 || port > 65535) {
    throw new Error("端口必须在 1024–65535 之间");
  }
  const config = loadConfig();
  config.port = port;
  saveConfig(config);
  return { port };
}

function main(): void {
  const command = process.argv[2] ?? "status";
  if (command === "status") {
    process.stdout.write(JSON.stringify(readBackgroundServiceState()));
    return;
  }
  if (command === "set-port") {
    process.stdout.write(JSON.stringify(setConfiguredServicePort(Number(process.argv[3]))));
    return;
  }
  if (command === "restore-disabled-autostart") {
    restoreDisabledAutostart();
    process.stdout.write(JSON.stringify(readBackgroundServiceState()));
    return;
  }
  throw new Error("未知的后台服务管理命令");
}

if (import.meta.main) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : "后台服务管理失败");
    process.exitCode = 1;
  }
}
