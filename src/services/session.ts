import { invoke } from "@tauri-apps/api/core";

export interface CodexSessionRecord {
  id: string;
  title: string;
  projectName: string;
  projectPath: string;
  path: string;
  updatedAt: number;
  messageCount?: number | null;
  charCount?: number | null;
  approximateTokens: number;
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
  resetHistoryProjectionCount?: number;
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

export function buildSingleSessionHistoryRepairOptions(
  sessionId: string,
  target: Pick<CodexSessionVisibilityRepairInstanceOption, "id" | "currentProvider">,
): CodexSessionVisibilityRepairOptions {
  const normalizedSessionId = sessionId.trim();
  const normalizedInstanceId = target.id.trim();
  if (!normalizedSessionId) throw new Error("会话 ID 不能为空");
  if (!normalizedInstanceId) throw new Error("Codex 实例 ID 不能为空");
  return {
    mode: "deep",
    targetProvider: target.currentProvider,
    targetInstanceId: normalizedInstanceId,
    repairInstanceIds: [normalizedInstanceId],
    sessionIds: [normalizedSessionId],
  };
}

export function listSessionsAcrossInstances(options: {
  titleQuery?: string;
  contentQuery?: string;
  instanceId?: string;
} = {}): Promise<CodexSessionRecord[]> {
  return invoke("codex_list_sessions_across_instances", {
    titleQuery: options.titleQuery?.trim() || null,
    contentQuery: options.contentQuery?.trim() || null,
    instanceId: options.instanceId || "default",
  });
}

export function getSessionTokenStatsAcrossInstances(
  sessionIds: string[],
  instanceId = "default",
): Promise<CodexSessionTokenStats[]> {
  return invoke("codex_get_session_token_stats_across_instances", { sessionIds, instanceId });
}

export function moveSessionsToTrashAcrossInstances(
  sessionIds: string[],
  instanceId = "default",
): Promise<CodexSessionTrashSummary> {
  return invoke("codex_move_sessions_to_trash_across_instances", { sessionIds, instanceId });
}

export function listTrashedSessionsAcrossInstances(
  instanceId = "default",
): Promise<CodexTrashedSessionRecord[]> {
  return invoke("codex_list_trashed_sessions_across_instances", { instanceId });
}

export function restoreSessionsFromTrashAcrossInstances(
  sessionIds: string[],
  instanceId = "default",
): Promise<CodexSessionTrashSummary> {
  return invoke("codex_restore_sessions_from_trash_across_instances", { sessionIds, instanceId });
}

export function copySessionHistoryAcrossInstances(
  sourceSessionId: string,
  copySuffix: string,
  targetProjectPath: string,
  sourceInstanceId = "default",
  targetInstanceId = "default",
): Promise<CodexSessionMutationResult> {
  return invoke("codex_copy_session_history_across_instances", {
    sourceSessionId,
    copySuffix,
    targetProjectPath,
    sourceInstanceId,
    targetInstanceId,
  });
}

export function renameSessionAcrossInstances(
  sessionId: string,
  title: string,
  instanceId = "default",
): Promise<CodexSessionMutationResult> {
  return invoke("codex_rename_session_across_instances", { sessionId, title, instanceId });
}

export function updateSessionWorkingDirectoryAcrossInstances(
  sessionId: string,
  projectPath: string,
  instanceId = "default",
): Promise<CodexSessionMutationResult> {
  return invoke("codex_update_session_working_directory_across_instances", {
    sessionId,
    projectPath,
    instanceId,
  });
}

export function listSessionContent(
  sessionId: string,
  cursor: number | null = null,
  limit = 20,
  direction: "asc" | "desc" = "asc",
  instanceId = "default",
): Promise<CodexSessionContentPage> {
  return invoke("codex_list_session_content", { sessionId, cursor, limit, direction, instanceId });
}

export function getSessionAsset(
  sessionId: string,
  assetId: string,
  instanceId = "default",
): Promise<CodexSessionAsset> {
  return invoke("codex_get_session_asset", { sessionId, assetId, instanceId });
}

export function deleteSessionTurn(
  sessionId: string,
  turnId: string,
  instanceId = "default",
): Promise<CodexSessionTurnMutationResult> {
  return invoke("codex_delete_session_turn", { sessionId, turnId, instanceId });
}

export function deleteSessionMessages(
  sessionId: string,
  turnId: string,
  messageIds: string[],
  instanceId = "default",
): Promise<CodexSessionMessageMutationResult> {
  return invoke("codex_delete_session_messages", { sessionId, turnId, messageIds, instanceId });
}

export function restoreSessionTurnBackup(
  sessionId: string,
  backupId: string,
  instanceId = "default",
): Promise<CodexSessionMutationResult> {
  return invoke("codex_restore_session_turn_backup", { sessionId, backupId, instanceId });
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
