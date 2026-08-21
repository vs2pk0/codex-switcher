import { diagnoseService } from "./node_modules/@bitkyc08/opencodex/src/service.ts";
import {
  loadConfig,
  saveConfig,
} from "./node_modules/@bitkyc08/opencodex/src/config.ts";

export function readBackgroundServiceState() {
  const diagnostic = diagnoseService();
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
  };
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
