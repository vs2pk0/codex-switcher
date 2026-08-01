export type ResetStatus =
  | "scheduled"
  | "running"
  | "completed"
  | "failed"
  | "missed"
  | "cancelled";

export type ResetLogType = "immediate" | "scheduled";

export type ResetLogResult = "success" | "failed" | "missed" | "cancelled";

export interface ScheduledReset {
  id: string;
  accountId: string;
  accountLabel: string;
  resetCreditId?: string;
  scheduledAt: number;
  status: ResetStatus;
  createdAt: number;
  startedAt?: number;
  finishedAt?: number;
  error?: string;
}

export interface ResetLog {
  id: string;
  accountId: string;
  accountLabel: string;
  type: ResetLogType;
  resetCreditId?: string;
  occurredAt: number;
  result: ResetLogResult;
  error?: string;
}

export interface ResetState {
  scheduledResets: ScheduledReset[];
  logs: ResetLog[];
}

export interface ResetClaim {
  state: ResetState;
  task?: ScheduledReset | null;
}
