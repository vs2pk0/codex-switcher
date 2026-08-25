import type { CodexSessionRecord } from "../services/session";

export type ActiveView =
  | "accounts"
  | "resets"
  | "sessions"
  | "usage"
  | "apiService"
  | "openCodex"
  | "instances"
  | "settings"
  | "pushSettings"
  | "about";

export interface SessionGroup {
  key: string;
  projectName: string;
  sessions: CodexSessionRecord[];
  latestUpdatedAt: number;
  approximateTokens: number;
  sizeBytes: number;
}
