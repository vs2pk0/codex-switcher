import { invoke } from "@tauri-apps/api/core";

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

export function launchCodexInstance(instanceId: string): Promise<CodexInstance> {
  return invoke("launch_codex_instance", { instanceId });
}

export function stopCodexInstance(instanceId: string): Promise<void> {
  return invoke("stop_codex_instance", { instanceId });
}

export function restartCodexInstance(instanceId: string): Promise<CodexInstance> {
  return invoke("restart_codex_instance", { instanceId });
}

export function instanceDisplayName(instance: CodexInstance): string {
  return instance.isDefault ? `${instance.name}（原版）` : instance.name;
}
