import { invoke } from "@tauri-apps/api/core";
import type {
  ResetClaim,
  ResetLog,
  ResetLogResult,
  ResetState,
  ScheduledReset,
} from "../types/reset";

export type {
  ResetLog,
  ResetLogResult,
  ResetLogType,
  ResetClaim,
  ResetState,
  ResetStatus,
  ScheduledReset,
} from "../types/reset";

export function getCodexResetState(): Promise<ResetState> {
  return invoke("get_codex_reset_state");
}

export function initializeCodexResetState(): Promise<ResetState> {
  return invoke("initialize_codex_reset_state");
}

export function createCodexScheduledReset(task: ScheduledReset): Promise<ResetState> {
  return invoke("create_codex_scheduled_reset", { task });
}

export function updateCodexScheduledReset(
  scheduleId: string,
  scheduledAt: number,
): Promise<ResetState> {
  return invoke("update_codex_scheduled_reset", { scheduleId, scheduledAt });
}

export function cancelCodexScheduledReset(
  scheduleId: string,
  occurredAt: number,
  logId: string,
): Promise<ResetState> {
  return invoke("cancel_codex_scheduled_reset", { scheduleId, occurredAt, logId });
}

export function claimCodexScheduledReset(scheduleId: string): Promise<ResetClaim> {
  return invoke("claim_codex_scheduled_reset", { scheduleId });
}

export function finishCodexScheduledReset(
  scheduleId: string,
  occurredAt: number,
  result: Extract<ResetLogResult, "success" | "failed">,
  error: string | undefined,
  logId: string,
): Promise<ResetState> {
  return invoke("finish_codex_scheduled_reset", {
    scheduleId,
    occurredAt,
    result,
    error,
    logId,
  });
}

export function appendCodexResetLog(log: ResetLog): Promise<ResetState> {
  return invoke("append_codex_reset_log", { log });
}

export function deleteCodexResetLog(logId: string): Promise<ResetState> {
  return invoke("delete_codex_reset_log", { logId });
}

export function clearCodexResetLogs(): Promise<ResetState> {
  return invoke("clear_codex_reset_logs");
}

export function formatResetCountdown(remainingMs: number): string {
  const totalSeconds = Math.max(0, Math.ceil(remainingMs / 1000));
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  const clock = [hours, minutes, seconds]
    .map((value) => String(value).padStart(2, "0"))
    .join(":");
  return days > 0 ? `${days}d ${clock}` : clock;
}
