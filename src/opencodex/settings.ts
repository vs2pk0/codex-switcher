import type { OpenCodexSettings } from "./types";

export const DEFAULT_OPEN_CODEX_PORT = 15800;
const OPEN_CODEX_SETTINGS_VERSION = 2;

interface StoredOpenCodexSettings {
  version?: number;
  port?: number;
  dashboardOpenMode?: OpenCodexSettings["dashboardOpenMode"];
}

export function normalizeOpenCodexSettings(value: unknown): OpenCodexSettings {
  const stored = value && typeof value === "object" ? value as StoredOpenCodexSettings : {};
  const dashboardOpenMode = stored.dashboardOpenMode === "browser" ? "browser" : "client";
  const port = stored.version === OPEN_CODEX_SETTINGS_VERSION
    && Number.isInteger(stored.port)
    && Number(stored.port) >= 1024
    && Number(stored.port) <= 65535
      ? Number(stored.port)
      : DEFAULT_OPEN_CODEX_PORT;
  return { port, dashboardOpenMode };
}

export function serializeOpenCodexSettings(settings: OpenCodexSettings): string {
  return JSON.stringify({ version: OPEN_CODEX_SETTINGS_VERSION, ...settings });
}

export function isOpenCodexPortPrompt(line: string): boolean {
  return /\bProxy port \[\d+\]:/.test(line);
}
