export type OpenCodexPage = "console" | "web" | "versions" | "logs" | "settings";

export type OpenCodexAction =
  | "init"
  | "start"
  | "stop"
  | "restart"
  | "status"
  | "doctor"
  | "sync"
  | "service_install"
  | "service_uninstall"
  | "restore"
  | "uninstall";

export interface OpenCodexSystemSnapshot {
  desktopVersion: string;
  engineVersion?: string | null;
  engineSource: "bundled" | "managed" | "missing";
  platform: string;
  installed: boolean;
  initialized: boolean;
  running: boolean;
  ready: boolean;
  pid?: number | null;
  port: number;
  dashboardUrl: string;
  integrationStatus: string;
  bundledRuntimeAvailable: boolean;
  backgroundService: OpenCodexBackgroundServiceState;
}

export interface OpenCodexBackgroundServiceState {
  supported: boolean;
  installed: boolean;
  enabled: boolean;
  running: boolean;
  viable: boolean;
  stale: boolean;
  conflict: boolean;
  backend?: string | null;
  summary: string;
}

export interface OpenCodexCommandStarted {
  operationId: string;
  interactive: boolean;
}

export interface OpenCodexCommandLogEvent {
  operationId: string;
  stream: "stdout" | "stderr" | "system" | string;
  line: string;
  timestamp: string;
}

export interface OpenCodexCommandFinishedEvent {
  operationId: string;
  action: string;
  success: boolean;
  exitCode?: number | null;
  message: string;
  timestamp: string;
}

export interface OpenCodexEngineRelease {
  version: string;
  tag: string;
  name: string;
  prerelease: boolean;
  publishedAt: string;
  url: string;
  newerThanCurrent: boolean;
  installed: boolean;
  active: boolean;
}

export interface OpenCodexEngineCatalog {
  currentVersion?: string | null;
  currentSource: string;
  latestStable?: OpenCodexEngineRelease | null;
  latestPreview?: OpenCodexEngineRelease | null;
  releases: OpenCodexEngineRelease[];
  installedVersions: string[];
}

export interface OpenCodexEngineInstallResult {
  version: string;
  source: string;
  message: string;
}

export interface OpenCodexSwitcherAccount {
  sourceId: string;
  targetAccountId: string;
  email: string;
  plan?: string | null;
  current: boolean;
  eligible: boolean;
  status: string;
  reason: string;
}

export interface OpenCodexSwitcherAccountScan {
  sourcePath: string;
  totalCount: number;
  eligibleCount: number;
  accounts: OpenCodexSwitcherAccount[];
}

export interface OpenCodexSwitcherImportResult {
  importedCount: number;
  skippedCount: number;
  imported: OpenCodexSwitcherAccount[];
  skipped: Array<{ sourceId: string; reason: string }>;
}

export interface OpenCodexSettings {
  port: number;
  dashboardOpenMode: "client" | "browser";
}
