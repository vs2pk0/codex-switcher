import { invoke } from "@tauri-apps/api/core";
export function transferOpenCodexData(sourceId: string, targetId: string, mode: "merge" | "overwrite", history: boolean, execute: boolean, fingerprint?: string): Promise<{
  rows: Array<{ name: string; source: number; target: number; result: number }>;
  backupPath: string | null; fingerprint: string; message: string;
}> {
  return invoke("opencodex_transfer_data", { sourceId, targetId, mode, history, execute, fingerprint });
}
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  OpenCodexAction,
  OpenCodexCommandFinishedEvent,
  OpenCodexCommandLogEvent,
  OpenCodexCommandStarted,
  OpenCodexEngineCatalog,
  OpenCodexEngineDeleteResult,
  OpenCodexEngineInstallResult,
  OpenCodexEngineProgress,
  OpenCodexSwitcherAccountScan,
  OpenCodexSwitcherDeleteResult,
  OpenCodexSwitcherImportResult,
  OpenCodexSystemSnapshot,
  OpenCodexImageGenerationSettings,
  OpenCodexImageGenerationUpdateResult,
  OpenCodexVisionModelCatalog,
  OpenCodexVisionModelsUpdateResult,
  OpenCodexVisionSidecarResponse,
  OpenCodexVisionSidecarSettings,
} from "./types";

export function getOpenCodexSnapshot(instanceId?: string): Promise<OpenCodexSystemSnapshot> {
  return invoke("opencodex_get_system_snapshot", { instanceId });
}

export function runOpenCodexAction(
  action: OpenCodexAction,
  port: number,
  instanceId?: string,
): Promise<OpenCodexCommandStarted> {
  return invoke("opencodex_run_action", { instanceId, request: { action, port, instanceId } });
}

export function writeOpenCodexInput(operationId: string, value: string, instanceId?: string): Promise<void> {
  return invoke("opencodex_write_command_input", { operationId, value, instanceId });
}

export function openOpenCodexDashboard(
  mode: "client" | "browser",
  port: number,
  instanceId?: string,
): Promise<void> {
  return invoke(
    mode === "client"
      ? "opencodex_open_dashboard_window"
      : "opencodex_open_dashboard_browser",
    { port, instanceId },
  );
}

export function readOpenCodexLogs(limit = 500, instanceId?: string): Promise<string[]> {
  return invoke("opencodex_read_manager_logs", { limit, instanceId });
}

export function getOpenCodexEngineCatalog(instanceId?: string): Promise<OpenCodexEngineCatalog> {
  return invoke("opencodex_get_engine_update_catalog", { instanceId });
}

export function installOpenCodexEngine(version: string, operationId: string, instanceId?: string, archivePath?: string): Promise<OpenCodexEngineInstallResult> {
  return invoke("opencodex_install_engine_version", { instanceId, request: { version, operationId, archivePath } });
}

export function subscribeOpenCodexEngineProgress(
  operationId: string,
  onProgress: (event: OpenCodexEngineProgress) => void,
): Promise<UnlistenFn> {
  return listen<OpenCodexEngineProgress>("opencodex-engine-progress", ({ payload }) => {
    if (payload.operationId === operationId) onProgress(payload);
  });
}

export function deleteOpenCodexEngine(version: string, instanceId?: string, removeData = false): Promise<OpenCodexEngineDeleteResult> {
  return invoke("opencodex_delete_engine_version", { instanceId, request: { version, removeData } });
}

export function scanOpenCodexSwitcherAccounts(instanceId?: string): Promise<OpenCodexSwitcherAccountScan> {
  return invoke("opencodex_scan_switcher_accounts", { instanceId });
}

export function importOpenCodexSwitcherAccounts(
  sourceIds: string[],
  instanceId?: string,
): Promise<OpenCodexSwitcherImportResult> {
  return invoke("opencodex_import_switcher_accounts", { instanceId, request: { sourceIds } });
}

export function bindOpenCodexSwitcherAccounts(
  sourceIds: string[],
  instanceId?: string,
): Promise<OpenCodexSwitcherImportResult> {
  return invoke("opencodex_bind_switcher_accounts", { instanceId, request: { sourceIds } });
}

export function deleteOpenCodexSwitcherAccount(
  sourceId: string,
  instanceId?: string,
): Promise<OpenCodexSwitcherDeleteResult> {
  return invoke("opencodex_delete_switcher_account", { instanceId, request: { sourceId } });
}

export function getOpenCodexVisionModels(instanceId?: string): Promise<OpenCodexVisionModelCatalog> {
  return invoke("opencodex_get_vision_models", { instanceId });
}

export function updateOpenCodexVisionModels(
  models: Array<{ provider: string; id: string }>,
  instanceId?: string,
): Promise<OpenCodexVisionModelsUpdateResult> {
  return invoke("opencodex_update_vision_models", { instanceId, request: { models, instanceId } });
}

export function getOpenCodexVisionSidecarSettings(
  instanceId?: string,
): Promise<OpenCodexVisionSidecarResponse> {
  return invoke("opencodex_get_vision_sidecar_settings", { instanceId });
}

export function updateOpenCodexVisionSidecarSettings(
  request: Pick<OpenCodexVisionSidecarSettings, "enabled" | "model" | "backend">,
  instanceId?: string,
): Promise<OpenCodexVisionSidecarResponse> {
  return invoke("opencodex_update_vision_sidecar_settings", { instanceId, request });
}

export function getOpenCodexImageGenerationSettings(
  instanceId?: string,
): Promise<OpenCodexImageGenerationSettings> {
  return invoke("opencodex_get_image_generation_settings", { instanceId });
}

export function updateOpenCodexImageGenerationSettings(
  request: { provider: string | null; timeoutMs: number | null },
  instanceId?: string,
): Promise<OpenCodexImageGenerationUpdateResult> {
  return invoke("opencodex_update_image_generation_settings", { instanceId, request });
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
