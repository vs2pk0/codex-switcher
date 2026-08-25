import { invoke } from "@tauri-apps/api/core";

export interface CodexSessionRecord {
  id: string;
  title: string;
  projectName: string;
  projectPath: string;
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

export interface CodexSessionMutationResult {
  sessionId: string;
  title: string;
  projectPath?: string | null;
  backupPath?: string | null;
  warnings: string[];
}

export interface CodexSessionContentPage {
  sessionId: string;
  cursor: number;
  nextCursor?: number | null;
  turns: CodexSessionTurn[];
}

export interface CodexSessionTurn {
  id: string;
  timestamp: string;
  messages: CodexSessionMessage[];
  technicalItemCount: number;
  canDelete: boolean;
}

export interface CodexSessionMessage {
  id: string;
  role: "user" | "assistant";
  phase: string;
  timestamp: string;
  text: string;
  attachments: CodexSessionAttachment[];
}

export interface CodexSessionAttachment {
  id: string;
  kind: "image" | "file" | "audio";
  name: string;
  sourcePath?: string | null;
  mimeType?: string | null;
  sizeBytes: number;
  available: boolean;
  inline: boolean;
}

export interface CodexSessionAsset {
  dataUrl: string;
  mimeType: string;
  sizeBytes: number;
}

export interface CodexSessionTurnMutationResult {
  sessionId: string;
  deletedTurnId: string;
  backupId: string;
  backupPath: string;
  removedBytes: number;
  warnings: string[];
}

export interface CodexSessionMessageMutationResult {
  sessionId: string;
  deletedMessageIds: string[];
  backupId: string;
  backupPath: string;
  removedBytes: number;
  warnings: string[];
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
  updatedCatalogRowCount?: number;
  verifiedVisibleSessionCount?: number;
  skippedNonSidebarSessionCount?: number;
  remainingInvisibleSessionCount?: number;
  createdLocalProjectCount?: number;
  assignedLocalProjectSessionCount?: number;
  verifiedLocalProjectCount?: number;
  skippedLocalProjectSessionCount?: number;
  recreatedGeneratedImageCount?: number;
  verifiedGeneratedImageCount?: number;
  invalidGeneratedImageCount?: number;
  desktopReloadRequired?: boolean;
  desktopReloadPerformed?: boolean;
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

export function copySessionHistoryAcrossInstances(
  sourceSessionId: string,
  copySuffix: string,
  targetProjectPath: string,
): Promise<CodexSessionMutationResult> {
  return invoke("codex_copy_session_history_across_instances", {
    sourceSessionId,
    copySuffix,
    targetProjectPath,
  });
}

export function renameSessionAcrossInstances(
  sessionId: string,
  title: string,
): Promise<CodexSessionMutationResult> {
  return invoke("codex_rename_session_across_instances", { sessionId, title });
}

export function updateSessionWorkingDirectoryAcrossInstances(
  sessionId: string,
  projectPath: string,
): Promise<CodexSessionMutationResult> {
  return invoke("codex_update_session_working_directory_across_instances", {
    sessionId,
    projectPath,
  });
}

export function listSessionContent(
  sessionId: string,
  cursor: number | null = null,
  limit = 20,
  direction: "asc" | "desc" = "asc",
): Promise<CodexSessionContentPage> {
  return invoke("codex_list_session_content", { sessionId, cursor, limit, direction });
}

export function getSessionAsset(
  sessionId: string,
  assetId: string,
): Promise<CodexSessionAsset> {
  return invoke("codex_get_session_asset", { sessionId, assetId });
}

export function deleteSessionTurn(
  sessionId: string,
  turnId: string,
): Promise<CodexSessionTurnMutationResult> {
  return invoke("codex_delete_session_turn", { sessionId, turnId });
}

export function deleteSessionMessages(
  sessionId: string,
  turnId: string,
  messageIds: string[],
): Promise<CodexSessionMessageMutationResult> {
  return invoke("codex_delete_session_messages", { sessionId, turnId, messageIds });
}

export function restoreSessionTurnBackup(
  sessionId: string,
  backupId: string,
): Promise<CodexSessionMutationResult> {
  return invoke("codex_restore_session_turn_backup", { sessionId, backupId });
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
