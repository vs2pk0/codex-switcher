import { invoke } from "@tauri-apps/api/core";

export interface CodexSessionRecord {
  id: string;
  title: string;
  projectName: string;
  path: string;
  updatedAt: number;
  messageCount: number;
  charCount: number;
  sizeBytes: number;
}

export interface CodexTrashedSessionRecord {
  id: string;
  title: string;
  originalPath: string;
  trashPath: string;
  deletedAt: number;
}

export interface CodexSessionTokenStats {
  sessionId: string;
  approximateTokens: number;
  charCount: number;
}

export interface CodexSessionTrashSummary {
  moved: number;
  restored: number;
  failed: string[];
}

export interface CodexSessionVisibilityRepairSummary {
  scanned: number;
  repaired: number;
  message: string;
  instanceCount?: number;
  mutatedInstanceCount?: number;
  changedRolloutFileCount?: number;
  updatedSqliteRowCount?: number;
  updatedSqliteTimestampRowCount?: number;
  addedSessionIndexEntryCount?: number;
  updatedSessionIndexEntryCount?: number;
  backupDirs?: string[];
  items?: CodexSessionVisibilityRepairItem[];
}

export type CodexSessionVisibilityRepairMode = "quick" | "deep";

export interface CodexSessionVisibilityRepairItem {
  instanceId: string;
  instanceName: string;
  targetProvider: string;
  changedRolloutFileCount: number;
  updatedSqliteRowCount: number;
  updatedSqliteTimestampRowCount: number;
  addedSessionIndexEntryCount: number;
  updatedSessionIndexEntryCount: number;
  backupDir?: string | null;
  running: boolean;
}

export interface CodexSessionVisibilityRepairInstanceOption {
  id: string;
  name: string;
  userDataDir: string;
  currentProvider: string;
  isDefault: boolean;
  running: boolean;
}

export interface CodexSessionVisibilityRepairInstanceList {
  defaultInstanceId: string;
  instances: CodexSessionVisibilityRepairInstanceOption[];
}

export interface CodexSessionVisibilityRepairProviderOption {
  id: string;
  sources: string[];
  isDefault: boolean;
}

export interface CodexSessionVisibilityRepairProviderList {
  defaultProvider: string;
  providers: CodexSessionVisibilityRepairProviderOption[];
}

export interface CodexSessionVisibilityRepairOptions {
  mode?: CodexSessionVisibilityRepairMode;
  targetProvider?: string | null;
  targetInstanceId?: string | null;
  repairInstanceIds?: string[] | null;
  sessionIds?: string[] | null;
}

export function listSessionsAcrossInstances(options: {
  titleQuery?: string;
  contentQuery?: string;
} = {}): Promise<CodexSessionRecord[]> {
  return invoke("codex_list_sessions_across_instances", {
    titleQuery: options.titleQuery?.trim() || null,
    contentQuery: options.contentQuery?.trim() || null,
  });
}

export function getSessionTokenStatsAcrossInstances(
  sessionIds: string[],
): Promise<CodexSessionTokenStats[]> {
  return invoke("codex_get_session_token_stats_across_instances", { sessionIds });
}

export function moveSessionsToTrashAcrossInstances(
  sessionIds: string[],
): Promise<CodexSessionTrashSummary> {
  return invoke("codex_move_sessions_to_trash_across_instances", { sessionIds });
}

export function listTrashedSessionsAcrossInstances(): Promise<CodexTrashedSessionRecord[]> {
  return invoke("codex_list_trashed_sessions_across_instances");
}

export function restoreSessionsFromTrashAcrossInstances(
  sessionIds: string[],
): Promise<CodexSessionTrashSummary> {
  return invoke("codex_restore_sessions_from_trash_across_instances", { sessionIds });
}

export function repairSessionVisibilityAcrossInstances(
  options: CodexSessionVisibilityRepairOptions = {},
): Promise<CodexSessionVisibilityRepairSummary> {
  return invoke("codex_repair_session_visibility_across_instances", {
    mode: options.mode ?? "quick",
    runId: null,
    targetProvider: options.targetProvider ?? null,
    targetInstanceId: options.targetInstanceId ?? null,
    repairInstanceIds: options.repairInstanceIds ?? null,
    sessionIds: options.sessionIds ?? null,
  });
}

export function listSessionVisibilityRepairInstances(): Promise<CodexSessionVisibilityRepairInstanceList> {
  return invoke("codex_list_session_visibility_repair_instances");
}

export function listSessionVisibilityRepairProviders(): Promise<CodexSessionVisibilityRepairProviderList> {
  return invoke("codex_list_session_visibility_repair_providers");
}

export function openPathInFileManager(path: string): Promise<void> {
  return invoke("open_path_in_file_manager", { path });
}
