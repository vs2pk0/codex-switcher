import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import {
  getConfigDir,
  loadConfig,
  saveConfigPreservingClaudeCode,
  withConfigMutationLockSync,
} from "./node_modules/@bitkyc08/opencodex/src/config.ts";
import {
  loadCodexAccountStore,
  removeCodexAccountCredential,
  saveCodexAccountCredential,
} from "./node_modules/@bitkyc08/opencodex/src/codex/account-store.ts";
import { deleteCodexAccount } from "./node_modules/@bitkyc08/opencodex/src/codex/account-lifecycle.ts";
import { runtimeRequest } from "./node_modules/@bitkyc08/opencodex/src/cli/runtime-api.ts";
import { findLiveProxy } from "./node_modules/@bitkyc08/opencodex/src/server/proxy-liveness.ts";
import { isValidCodexAccountId } from "./node_modules/@bitkyc08/opencodex/src/codex/account-id.ts";
import { withCodexAccountLogLabel } from "./node_modules/@bitkyc08/opencodex/src/codex/account-label.ts";
import { appendDefaultCodexAccountNamespace } from "./node_modules/@bitkyc08/opencodex/src/codex/account-namespaces.ts";
import {
  decodeJwtPayload,
  extractAccountId,
  extractEmail,
} from "./node_modules/@bitkyc08/opencodex/src/oauth/chatgpt.ts";
import type {
  CodexAccountCredentials,
  OcxConfig,
} from "./node_modules/@bitkyc08/opencodex/src/types.ts";

const MAX_SOURCE_BYTES = 16 * 1024 * 1024;
const MAX_SOURCE_ACCOUNTS = 2_000;
const MAX_IMPORT_SELECTION = 1_000;

interface SwitcherTokens {
  access_token?: unknown;
  refresh_token?: unknown;
  id_token?: unknown;
}

interface SwitcherAccount {
  id?: unknown;
  email?: unknown;
  account_name?: unknown;
  is_hidden?: unknown;
  auth_mode?: unknown;
  openai_api_key?: unknown;
  plan_type?: unknown;
  access_token_expires_at?: unknown;
  tokens?: unknown;
}

interface SwitcherStore {
  accounts: SwitcherAccount[];
  current_account_id?: unknown;
}

interface ExistingState {
  accountIds: Set<string>;
  accountIdentityByTargetId: Map<string, { email?: string; chatgptAccountId?: string }>;
  switcherSourceByTargetId: Map<string, string>;
  legacySwitcherMigrationEnabled: boolean;
  chatgptAccountIds: Set<string>;
}

interface ImportMaterial {
  sourceId: string;
  targetAccountId: string;
  email: string;
  plan?: string;
  credential: CodexAccountCredentials;
}

export interface SwitcherAccountSummary {
  sourceId: string;
  targetAccountId: string;
  email: string;
  plan: string | null;
  current: boolean;
  eligible: boolean;
  deletable: boolean;
  status: "ready" | "already_imported" | "unsupported" | "invalid";
  reason: string;
}

export interface SwitcherAccountScan {
  sourcePath: string;
  totalCount: number;
  eligibleCount: number;
  accounts: SwitcherAccountSummary[];
}

export interface SwitcherImportResult {
  importedCount: number;
  skippedCount: number;
  imported: SwitcherAccountSummary[];
  skipped: Array<{ sourceId: string; reason: string }>;
}

export interface SwitcherDeleteResult {
  sourceId: string;
  targetAccountId: string;
  deleted: boolean;
  message: string;
}

interface SwitcherDeleteRuntimeDeps {
  findLiveProxy?: typeof findLiveProxy;
  runtimeRequest?: typeof runtimeRequest;
}

function text(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  return trimmed || undefined;
}

function tokensOf(account: SwitcherAccount): SwitcherTokens {
  return account.tokens && typeof account.tokens === "object" && !Array.isArray(account.tokens)
    ? account.tokens as SwitcherTokens
    : {};
}

function defaultSourcePath(): string {
  return join(homedir(), ".codex_switcher", "account", "accounts.json");
}

function readJsonObject(path: string): Record<string, unknown> {
  if (!existsSync(path)) return {};
  const value: unknown = JSON.parse(readFileSync(path, "utf8"));
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`文件不是 JSON 对象：${path}`);
  }
  return value as Record<string, unknown>;
}

function readSwitcherStore(sourcePath: string): SwitcherStore {
  const metadata = statSync(sourcePath);
  if (!metadata.isFile()) throw new Error("Codex Switcher 账号路径不是文件");
  if (metadata.size > MAX_SOURCE_BYTES) throw new Error("Codex Switcher 账号文件超过 16 MiB 安全限制");
  const value: unknown = JSON.parse(readFileSync(sourcePath, "utf8"));
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Codex Switcher 账号文件格式无效");
  }
  const accounts = (value as { accounts?: unknown }).accounts;
  if (!Array.isArray(accounts)) throw new Error("Codex Switcher 账号文件缺少 accounts 数组");
  if (accounts.length > MAX_SOURCE_ACCOUNTS) throw new Error("Codex Switcher 账号数量超过 2000 条安全限制");
  return {
    accounts: accounts as SwitcherAccount[],
    current_account_id: (value as { current_account_id?: unknown }).current_account_id,
  };
}

function existingStateFromRaw(configDir: string): ExistingState {
  const config = readJsonObject(join(configDir, "config.json"));
  const credentials = readJsonObject(join(configDir, "codex-accounts.json"));
  const accountIds = new Set<string>();
  const accountIdentityByTargetId = new Map<string, { email?: string; chatgptAccountId?: string }>();
  const switcherSourceByTargetId = new Map<string, string>();
  const sourceMap = config.codexSwitcherSources;
  if (sourceMap && typeof sourceMap === "object" && !Array.isArray(sourceMap)) {
    for (const [targetId, sourceId] of Object.entries(sourceMap as Record<string, unknown>)) {
      const normalized = text(sourceId);
      if (normalized) switcherSourceByTargetId.set(targetId, normalized);
    }
  }
  const chatgptAccountIds = new Set<string>();

  if (Array.isArray(config.codexAccounts)) {
    for (const account of config.codexAccounts) {
      if (account && typeof account === "object" && !Array.isArray(account)) {
        const id = text((account as { id?: unknown }).id);
        if (id) {
          accountIds.add(id);
          accountIdentityByTargetId.set(id, {
            email: text((account as { email?: unknown }).email)?.toLowerCase(),
          });
        }
      }
    }
  }
  for (const [id, record] of Object.entries(credentials)) {
    if (!record || typeof record !== "object" || Array.isArray(record)) continue;
    const credential = (record as { credential?: unknown }).credential;
    if (!credential || typeof credential !== "object" || Array.isArray(credential)) continue;
    accountIds.add(id);
    const chatgptAccountId = text((credential as { chatgptAccountId?: unknown }).chatgptAccountId);
    if (chatgptAccountId) chatgptAccountIds.add(chatgptAccountId);
    const identity = accountIdentityByTargetId.get(id) ?? {};
    if (chatgptAccountId) identity.chatgptAccountId = chatgptAccountId;
    accountIdentityByTargetId.set(id, identity);
  }
  const legacySwitcherMigrationEnabled = accountIds.size > 0
    && [...accountIds].filter(id => id.startsWith("switcher-")).length >= 2;
  return {
    accountIds,
    accountIdentityByTargetId,
    switcherSourceByTargetId,
    legacySwitcherMigrationEnabled,
    chatgptAccountIds,
  };
}

function existingStateFromConfig(config: OcxConfig): ExistingState {
  const credentials = loadCodexAccountStore();
  const configuredAccounts = config.codexAccounts ?? [];
  const configuredAccountIds = configuredAccounts.map(account => account.id);
  const accountIdentityByTargetId = new Map(
    configuredAccounts.map(account => [account.id, {
      email: account.email?.trim().toLowerCase(),
      chatgptAccountId: credentials[account.id]?.chatgptAccountId?.trim(),
    }]),
  );
  const switcherSourceByTargetId = new Map<string, string>();
  const sourceMap = (config as OcxConfig & { codexSwitcherSources?: unknown }).codexSwitcherSources;
  if (sourceMap && typeof sourceMap === "object" && !Array.isArray(sourceMap)) {
    for (const [targetId, sourceId] of Object.entries(sourceMap as Record<string, unknown>)) {
      const normalized = text(sourceId);
      if (normalized) switcherSourceByTargetId.set(targetId, normalized);
    }
  }
  const legacySwitcherMigrationEnabled = new Set([
    ...configuredAccountIds,
    ...Object.keys(credentials),
  ]).size > 0
    && [...new Set([...configuredAccountIds, ...Object.keys(credentials)])]
      .filter(id => id.startsWith("switcher-")).length >= 2;
  return {
    accountIds: new Set([
      ...configuredAccountIds,
      ...Object.keys(credentials),
    ]),
    accountIdentityByTargetId,
    switcherSourceByTargetId,
    legacySwitcherMigrationEnabled,
    chatgptAccountIds: new Set(
      Object.values(credentials)
        .map(credential => credential.chatgptAccountId?.trim())
        .filter((value): value is string => !!value),
    ),
  };
}

function maskEmail(email: string): string {
  const at = email.indexOf("@");
  if (at <= 0) return email.length <= 2 ? "***" : `${email[0]}***${email.at(-1)}`;
  const local = email.slice(0, at);
  const domain = email.slice(at + 1);
  const visible = local.length <= 1 ? local[0] ?? "" : `${local[0]}***${local.at(-1)}`;
  return `${visible}@${domain}`;
}

function targetAccountId(sourceId: string): string {
  const direct = `switcher-${sourceId}`;
  if (isValidCodexAccountId(direct)) return direct;
  return `switcher-${createHash("sha256").update(sourceId).digest("hex").slice(0, 24)}`;
}

function exactSwitcherMigrationExistsInRawState(
  sourceAccount: SwitcherAccount | undefined,
  sourceId: string,
  configDir = getConfigDir(),
): boolean {
  if (!sourceAccount) return false;
  return inspectAccount(
    sourceAccount,
    undefined,
    existingStateFromRaw(configDir),
    new Set<string>(),
  ).summary.deletable;
}

function expiresAtMs(account: SwitcherAccount, accessToken: string): number {
  const sourceExpiry = text(account.access_token_expires_at);
  if (sourceExpiry) {
    const parsed = Date.parse(sourceExpiry);
    if (Number.isFinite(parsed)) return parsed;
  }
  const exp = decodeJwtPayload(accessToken)?.exp;
  return typeof exp === "number" && Number.isFinite(exp) && exp > 0 ? exp * 1000 : 0;
}

function inspectAccount(
  account: SwitcherAccount,
  currentAccountId: string | undefined,
  existing: ExistingState,
  seenSourceIdentities: Set<string>,
): { summary: SwitcherAccountSummary; material?: ImportMaterial } {
  const sourceId = text(account.id) ?? "";
  const targetId = sourceId ? targetAccountId(sourceId) : "";
  const tokenSet = tokensOf(account);
  const accessToken = text(tokenSet.access_token);
  const refreshToken = text(tokenSet.refresh_token);
  const idToken = text(tokenSet.id_token);
  const email = (text(account.email) ?? extractEmail(idToken, accessToken) ?? "").toLowerCase();
  const plan = text(account.plan_type) ?? null;
  const base = {
    sourceId,
    targetAccountId: targetId,
    email: email ? maskEmail(email) : "未提供邮箱",
    plan,
    current: !!sourceId && sourceId === currentAccountId,
    deletable: false,
  };
  const reject = (
    status: SwitcherAccountSummary["status"],
    reason: string,
  ): { summary: SwitcherAccountSummary } => ({
    summary: { ...base, eligible: false, status, reason },
  });

  if (!sourceId || !targetId) return reject("invalid", "账号 ID 无效");
  if (existing.accountIds.has(targetId)) {
    const identity = existing.accountIdentityByTargetId.get(targetId);
    const mappedSourceId = existing.switcherSourceByTargetId.get(targetId);
    const sourceChatgptAccountId = extractAccountId(idToken, accessToken);
    const provenanceMatches = mappedSourceId === sourceId
      || (!mappedSourceId && existing.legacySwitcherMigrationEnabled);
    return {
      summary: {
        ...base,
        eligible: false,
        deletable: provenanceMatches
          && !!identity
          && !!sourceChatgptAccountId
          && identity.email === email
          && identity.chatgptAccountId === sourceChatgptAccountId,
        status: "already_imported",
        reason: "已经导入过此 Switcher 账号",
      },
    };
  }
  if (account.is_hidden === true) return reject("unsupported", "账号已启用隐身模式，不能导入 OpenCodex");
  if (!accessToken) {
    return text(account.openai_api_key) || text(account.auth_mode)
      ? reject("unsupported", "API Key 账号不属于 Codex OAuth 账号池")
      : reject("invalid", "缺少 access_token");
  }
  if (!refreshToken) return reject("unsupported", "缺少 refresh_token，无法自动续期");
  const chatgptAccountId = extractAccountId(idToken, accessToken);
  if (!chatgptAccountId) return reject("invalid", "令牌中缺少 ChatGPT Account ID");
  if (!email || email.length > 320 || /[\x00-\x1f\x7f]/.test(email)) {
    return reject("invalid", "账号邮箱无效");
  }
  if (existing.chatgptAccountIds.has(chatgptAccountId)) {
    return reject("already_imported", "相同 ChatGPT 身份已存在于 OpenCodex");
  }
  if (seenSourceIdentities.has(chatgptAccountId)) {
    return reject("already_imported", "Switcher 源文件中存在重复 ChatGPT 身份");
  }
  seenSourceIdentities.add(chatgptAccountId);

  return {
    summary: { ...base, eligible: true, status: "ready", reason: "可以导入" },
    material: {
      sourceId,
      targetAccountId: targetId,
      email,
      ...(plan ? { plan } : {}),
      credential: {
        accessToken,
        refreshToken,
        expiresAt: expiresAtMs(account, accessToken),
        chatgptAccountId,
      },
    },
  };
}

function inspectStore(store: SwitcherStore, existing: ExistingState) {
  const currentAccountId = text(store.current_account_id);
  const seenSourceIdentities = new Set<string>();
  return store.accounts.map(account => inspectAccount(account, currentAccountId, existing, seenSourceIdentities));
}

export function scanSwitcherAccounts(
  sourcePath = defaultSourcePath(),
  configDir = getConfigDir(),
): SwitcherAccountScan {
  const store = readSwitcherStore(sourcePath);
  const inspected = inspectStore(store, existingStateFromRaw(configDir));
  const accounts = inspected.map(item => item.summary);
  return {
    sourcePath,
    totalCount: accounts.length,
    eligibleCount: accounts.filter(account => account.eligible).length,
    accounts,
  };
}

function validateSelection(sourceIds: unknown): string[] {
  if (!Array.isArray(sourceIds) || sourceIds.length === 0) throw new Error("请至少选择一个账号");
  if (sourceIds.length > MAX_IMPORT_SELECTION) throw new Error("单次最多导入 1000 个账号");
  const selected = sourceIds.map(value => {
    if (typeof value !== "string" || value.length === 0 || value.length > 128) {
      throw new Error("导入请求包含无效账号 ID");
    }
    return value;
  });
  return [...new Set(selected)];
}

export function importSwitcherAccounts(
  sourceIds: unknown,
  sourcePath = defaultSourcePath(),
): SwitcherImportResult {
  const selected = validateSelection(sourceIds);
  const selectedSet = new Set(selected);
  const store = readSwitcherStore(sourcePath);

  return withConfigMutationLockSync(() => {
    const config = loadConfig();
    const previousConfig = structuredClone(config);
    const existing = existingStateFromConfig(config);
    const inspected = inspectStore(store, existing);
    const selectedRows = inspected.filter(item => selectedSet.has(item.summary.sourceId));
    const foundIds = new Set(selectedRows.map(item => item.summary.sourceId));
    const skipped = selected
      .filter(sourceId => !foundIds.has(sourceId))
      .map(sourceId => ({ sourceId, reason: "源文件中不存在此账号" }));
    const materials: ImportMaterial[] = [];
    for (const row of selectedRows) {
      if (row.material) materials.push(row.material);
      else skipped.push({ sourceId: row.summary.sourceId, reason: row.summary.reason });
    }
    if (materials.length === 0) {
      return { importedCount: 0, skippedCount: skipped.length, imported: [], skipped };
    }

    const accounts = [...(config.codexAccounts ?? [])];
    const importedSummaries: SwitcherAccountSummary[] = [];
    const importedIds: string[] = [];
    for (const material of materials) {
      const account = withCodexAccountLogLabel({
        id: material.targetAccountId,
        email: material.email,
        ...(material.plan ? { plan: material.plan } : {}),
        isMain: false,
      }, accounts);
      accounts.push(account);
      config.codexAccounts = accounts;
      const sourceMap = ((config as OcxConfig & { codexSwitcherSources?: Record<string, string> }).codexSwitcherSources
        ??= {});
      sourceMap[material.targetAccountId] = material.sourceId;
      if (config.codexAccountNamespaces && Object.keys(config.codexAccountNamespaces).length > 0) {
        appendDefaultCodexAccountNamespace(config, account);
      }
      importedIds.push(material.targetAccountId);
      importedSummaries.push({
        sourceId: material.sourceId,
        targetAccountId: material.targetAccountId,
        email: maskEmail(material.email),
        plan: material.plan ?? null,
        current: false,
        eligible: false,
        deletable: true,
        status: "already_imported",
        reason: "导入成功",
      });
    }

    saveConfigPreservingClaudeCode(config);
    try {
      for (const material of materials) {
        saveCodexAccountCredential(material.targetAccountId, material.credential);
      }
    } catch (error) {
      for (const accountId of importedIds) {
        try { removeCodexAccountCredential(accountId); } catch { /* best-effort rollback */ }
      }
      try { saveConfigPreservingClaudeCode(previousConfig); } catch { /* preserve primary error */ }
      throw error;
    }

    return {
      importedCount: importedSummaries.length,
      skippedCount: skipped.length,
      imported: importedSummaries,
      skipped,
    };
  });
}

export function deleteSwitcherAccount(
  sourceIdInput: unknown,
  sourcePath = defaultSourcePath(),
): SwitcherDeleteResult {
  if (typeof sourceIdInput !== "string" || sourceIdInput.length === 0 || sourceIdInput.length > 128) {
    throw new Error("删除请求包含无效账号 ID");
  }
  const sourceId = sourceIdInput;
  const store = readSwitcherStore(sourcePath);
  const sourceAccount = store.accounts.find(account => text(account.id) === sourceId);

  const targetId = targetAccountId(sourceId);
  const targetExists = exactSwitcherMigrationExistsInRawState(sourceAccount, sourceId);
  const config = loadConfig();
  if (!targetExists) {
    const conflictingTargetExists = (config.codexAccounts ?? [])
      .some(account => !account.isMain && account.id === targetId)
      || !!loadCodexAccountStore()[targetId];
    return {
      sourceId,
      targetAccountId: targetId,
      deleted: false,
      message: conflictingTargetExists
        ? "检测到同 ID 的 OpenCodex 账号，但身份不匹配，已阻止删除"
        : "OpenCodex 中没有此 Switcher 迁移账号",
    };
  }

  deleteCodexAccount(config, targetId);
  return {
    sourceId,
    targetAccountId: targetId,
    deleted: true,
    message: "已从 OpenCodex 删除账号，Switcher 原账号仍保留",
  };
}

export async function deleteSwitcherAccountForCurrentRuntime(
  sourceIdInput: unknown,
  sourcePath = defaultSourcePath(),
  deps: SwitcherDeleteRuntimeDeps = {},
): Promise<SwitcherDeleteResult> {
  if (typeof sourceIdInput !== "string" || sourceIdInput.length === 0 || sourceIdInput.length > 128) {
    throw new Error("删除请求包含无效账号 ID");
  }
  const sourceId = sourceIdInput;
  const store = readSwitcherStore(sourcePath);
  const sourceAccount = store.accounts.find(account => text(account.id) === sourceId);
  const targetId = targetAccountId(sourceId);
  const targetExists = exactSwitcherMigrationExistsInRawState(sourceAccount, sourceId);
  const config = loadConfig();
  if (!targetExists) {
    const conflictingTargetExists = (config.codexAccounts ?? [])
      .some(account => !account.isMain && account.id === targetId)
      || !!loadCodexAccountStore()[targetId];
    return {
      sourceId,
      targetAccountId: targetId,
      deleted: false,
      message: conflictingTargetExists
        ? "检测到同 ID 的 OpenCodex 账号，但身份不匹配，已阻止删除"
        : "OpenCodex 中没有此 Switcher 迁移账号",
    };
  }

  if (await (deps.findLiveProxy ?? findLiveProxy)()) {
    await (deps.runtimeRequest ?? runtimeRequest)(
      `/api/codex-auth/accounts?id=${encodeURIComponent(targetId)}`,
      { method: "DELETE" },
    );
  } else {
    return deleteSwitcherAccount(sourceId, sourcePath);
  }
  return {
    sourceId,
    targetAccountId: targetId,
    deleted: true,
    message: "已从 OpenCodex 删除账号，Switcher 原账号仍保留",
  };
}

async function readStdin(): Promise<string> {
  return await new Response(Bun.stdin.stream()).text();
}

async function main() {
  const command = process.argv[2];
  if (command === "scan") {
    console.log(JSON.stringify(scanSwitcherAccounts()));
    return;
  }
  if (command === "import") {
    const input = JSON.parse(await readStdin()) as { sourceIds?: unknown };
    console.log(JSON.stringify(importSwitcherAccounts(input.sourceIds)));
    return;
  }
  if (command === "delete") {
    const input = JSON.parse(await readStdin()) as { sourceId?: unknown };
    console.log(JSON.stringify(await deleteSwitcherAccountForCurrentRuntime(input.sourceId)));
    return;
  }
  throw new Error("未知的 Switcher 导入命令");
}

if (import.meta.main) {
  main().catch(error => {
    console.error(error instanceof Error ? error.message : "Switcher 账号处理失败");
    process.exitCode = 1;
  });
}
