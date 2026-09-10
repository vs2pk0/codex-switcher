import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";

export const DEFAULT_CODEX_INSTANCE_ID = "default";

export interface CodexInstance {
  id: string;
  name: string;
  codexHome: string;
  electronData: string;
  appPath: string;
  workspace?: string | null;
  createdAt: number;
  isDefault: boolean;
  running: boolean;
  pid?: number | null;
  openCodexConnected: boolean;
  /** config.toml 的 model_provider 是否指向本地 API 服务（CLIProxyAPI）。 */
  apiServiceConnected: boolean;
}

export interface CodexInstanceCapabilities {
  managedInstancesSupported: boolean;
}

export interface SaveCodexInstanceInput {
  id?: string | null;
  name: string;
  codexHome?: string | null;
  electronData?: string | null;
  appPath?: string | null;
  workspace?: string | null;
}

export interface DeleteCodexInstanceResult {
  instanceId: string;
  instanceName: string;
  deletedPaths: string[];
  deletedBackupCount: number;
}

export function listCodexInstances(): Promise<CodexInstance[]> {
  return invoke("list_codex_instances");
}

export function getCodexInstanceCapabilities(): Promise<CodexInstanceCapabilities> {
  return invoke("get_codex_instance_capabilities");
}

export function saveCodexInstance(input: SaveCodexInstanceInput): Promise<CodexInstance> {
  return invoke("save_codex_instance", { input });
}

export function deleteCodexInstance(instanceId: string): Promise<DeleteCodexInstanceResult> {
  return invoke("delete_codex_instance", { instanceId });
}

export interface CodexInstanceLaunchResult {
  instance: CodexInstance;
  /** 本次为实例顺带拉起的依赖服务（OpenCodex / API 服务）。 */
  startedServices: string[];
}

/** 启动实例；若实例已接入 OpenCodex 或 API 服务而服务未运行，会先启动服务再启动 Codex。 */
export function launchCodexInstance(instanceId: string): Promise<CodexInstanceLaunchResult> {
  return invoke("launch_codex_instance_with_services", { instanceId });
}

export function stopCodexInstance(instanceId: string): Promise<void> {
  return invoke("stop_codex_instance", { instanceId });
}

export function restartCodexInstance(instanceId: string): Promise<CodexInstance> {
  return invoke("restart_codex_instance", { instanceId });
}

export function instanceDisplayName(instance: CodexInstance): string {
  return instance.isDefault ? t("系统默认实例（原版）") : instance.name;
}
