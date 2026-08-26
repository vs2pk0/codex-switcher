import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  OpenCodexAction,
  OpenCodexCommandFinishedEvent,
  OpenCodexCommandLogEvent,
  OpenCodexCommandStarted,
  OpenCodexEngineCatalog,
  OpenCodexEngineDeleteResult,
  OpenCodexEngineInstallResult,
  OpenCodexSwitcherAccountScan,
  OpenCodexSwitcherDeleteResult,
  OpenCodexSwitcherImportResult,
  OpenCodexSystemSnapshot,
  OpenCodexVisionModelCatalog,
  OpenCodexVisionModelsUpdateResult,
} from "./types";

export function getOpenCodexSnapshot(): Promise<OpenCodexSystemSnapshot> {
  return invoke("opencodex_get_system_snapshot");
}

export function runOpenCodexAction(
  action: OpenCodexAction,
  port: number,
  instanceId?: string,
): Promise<OpenCodexCommandStarted> {
  return invoke("opencodex_run_action", { request: { action, port, instanceId } });
}

export function writeOpenCodexInput(operationId: string, value: string): Promise<void> {
  return invoke("opencodex_write_command_input", { operationId, value });
}

export function openOpenCodexDashboard(
  mode: "client" | "browser",
  port: number,
): Promise<void> {
  return invoke(
    mode === "client"
      ? "opencodex_open_dashboard_window"
      : "opencodex_open_dashboard_browser",
    { port },
  );
}

export function readOpenCodexLogs(limit = 500): Promise<string[]> {
  return invoke("opencodex_read_manager_logs", { limit });
}

export function getOpenCodexEngineCatalog(): Promise<OpenCodexEngineCatalog> {
  return invoke("opencodex_get_engine_update_catalog");
}

export function installOpenCodexEngine(version: string): Promise<OpenCodexEngineInstallResult> {
  return invoke("opencodex_install_engine_version", { request: { version } });
}

export function activateBundledOpenCodexEngine(): Promise<OpenCodexEngineInstallResult> {
  return invoke("opencodex_activate_bundled_engine");
}

export function deleteOpenCodexEngine(version: string): Promise<OpenCodexEngineDeleteResult> {
  return invoke("opencodex_delete_engine_version", { request: { version } });
}

export function scanOpenCodexSwitcherAccounts(): Promise<OpenCodexSwitcherAccountScan> {
  return invoke("opencodex_scan_switcher_accounts");
}

export function importOpenCodexSwitcherAccounts(
  sourceIds: string[],
): Promise<OpenCodexSwitcherImportResult> {
  return invoke("opencodex_import_switcher_accounts", { request: { sourceIds } });
}

export function bindOpenCodexSwitcherAccounts(
  sourceIds: string[],
): Promise<OpenCodexSwitcherImportResult> {
  return invoke("opencodex_bind_switcher_accounts", { request: { sourceIds } });
}

export function deleteOpenCodexSwitcherAccount(
  sourceId: string,
): Promise<OpenCodexSwitcherDeleteResult> {
  return invoke("opencodex_delete_switcher_account", { request: { sourceId } });
}

export function getOpenCodexVisionModels(): Promise<OpenCodexVisionModelCatalog> {
  return invoke("opencodex_get_vision_models");
}

export function updateOpenCodexVisionModels(
  models: Array<{ provider: string; id: string }>,
): Promise<OpenCodexVisionModelsUpdateResult> {
  return invoke("opencodex_update_vision_models", { request: { models } });
}

export async function subscribeOpenCodexEvents(
  onLog: (event: OpenCodexCommandLogEvent) => void,
  onFinished: (event: OpenCodexCommandFinishedEvent) => void,
): Promise<UnlistenFn> {
  const unlistenLog = await listen<OpenCodexCommandLogEvent>("opencodex-command-log", (event) =>
    onLog(event.payload),
  );
  const unlistenFinished = await listen<OpenCodexCommandFinishedEvent>(
    "opencodex-command-finished",
    (event) => onFinished(event.payload),
  );
  return () => {
    unlistenLog();
    unlistenFinished();
  };
}
