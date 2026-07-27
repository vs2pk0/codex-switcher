import type { CodexSessionRecord } from "../services/session";

export type ActiveView =
  | "accounts"
  | "sessions"
  | "usage"
  | "apiService"
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
