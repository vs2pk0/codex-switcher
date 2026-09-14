import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  cleanupExpiredPoolAccounts,
  deleteSwitcherAccount,
  deleteSwitcherAccountForCurrentRuntime,
  importSwitcherAccounts,
  importSwitcherAccountsForCurrentRuntime,
  scanSwitcherAccounts,
} from "./manager-switcher-import.ts";

const temporaryRoots: string[] = [];
const originalOpenCodexHome = process.env.OPENCODEX_HOME;

function temporaryRoot(): string {
  const root = mkdtempSync(join(tmpdir(), "opencodex-manager-switcher-"));
  temporaryRoots.push(root);
  return root;
}

function jwt(payload: Record<string, unknown>): string {
  return `${Buffer.from("{}").toString("base64url")}.${Buffer.from(JSON.stringify(payload)).toString("base64url")}.signature`;
}

afterEach(() => {
  if (originalOpenCodexHome === undefined) delete process.env.OPENCODEX_HOME;
  else process.env.OPENCODEX_HOME = originalOpenCodexHome;
  for (const root of temporaryRoots.splice(0)) rmSync(root, { recursive: true, force: true });
});

describe("Codex Switcher account import", () => {
  test("online OAuth is skipped and runtime failures do not expose credentials", async () => {
    const root = temporaryRoot();
    process.env.OPENCODEX_HOME = root;
    const source = join(root, "accounts.json");
    writeFileSync(join(root, "config.json"), JSON.stringify({ providers: {} }));
    writeFileSync(source, JSON.stringify({ accounts: [
      { id: "oauth", email: "test@example.com", tokens: { access_token: jwt({ exp: 4102444800, "https://api.openai.com/auth": { chatgpt_account_id: "test" } }), refresh_token: "refresh" } },
      { id: "api", openai_api_key: "private-fixture", api_base_url: "https://example.com/v1" },
    ] }));
    let requests = 0;
    const result = await importSwitcherAccountsForCurrentRuntime(["oauth", "api"], source, {
      findLiveProxy: async () => ({ pid: 42, port: 15800, source: "runtime" }),
      runtimeRequest: async () => { requests++; throw new Error("private-fixture"); },
    });
    expect(requests).toBe(1);
    expect(result.importedCount).toBe(0);
    expect(result.skippedCount).toBe(2);
    expect(result.skipped[0].reason).toContain("OAuth");
    expect(JSON.stringify(result)).not.toContain("private-fixture");
  });
  test("running API import and delete use runtime routes without overwriting unrelated config", async () => {
    const root = temporaryRoot();
    process.env.OPENCODEX_HOME = root;
    const source = join(root, "accounts.json");
    const configPath = join(root, "config.json");
    writeFileSync(source, JSON.stringify({ accounts: [{ id: "online", openai_api_key: "fixture-secret", api_base_url: "https://example.com/v1" }] }));
    writeFileSync(configPath, JSON.stringify({ providers: {} }));
    const calls: string[] = [];
    const deps = {
      findLiveProxy: async () => ({ pid: 42, port: 15800, source: "runtime" as const }),
      runtimeRequest: async (path: string, init: RequestInit) => {
        calls.push(`${init.method} ${path}`);
        const config = JSON.parse(readFileSync(configPath, "utf8"));
        if (init.method === "POST") {
          const body = JSON.parse(init.body as string);
          expect(body.setDefault).toBe(false);
          config.providers[body.name] = { ...body.provider, name: "Engine enriched" };
          config.concurrentSetting = "preserve";
        } else {
          delete config.providers[new URL(path, "http://localhost").searchParams.get("name")!];
        }
        writeFileSync(configPath, JSON.stringify(config));
        return { ok: true };
      },
    };
    expect((await importSwitcherAccountsForCurrentRuntime(["online"], source, deps)).importedCount).toBe(1);
    expect(scanSwitcherAccounts(source).accounts[0].deletable).toBe(true);
    expect((await importSwitcherAccountsForCurrentRuntime(["online"], source, deps)).skippedCount).toBe(1);
    expect((await deleteSwitcherAccountForCurrentRuntime("online", source, deps)).deleted).toBe(true);
    expect(calls.length).toBe(2);
    expect(calls[0]).toBe("POST /api/providers");
    expect(calls[1]).toStartWith("DELETE /api/providers?name=");
    expect(JSON.parse(readFileSync(configPath, "utf8")).concurrentSetting).toBe("preserve");
    expect(JSON.parse(readFileSync(join(root, "switcher-provider-sources.json"), "utf8"))).toEqual({});
    expect(JSON.parse(readFileSync(source, "utf8")).accounts.length).toBe(1);
  });
  test("startup cleanup removes only terminal pool accounts in the target home", async () => {
    const root = temporaryRoot();
    process.env.OPENCODEX_HOME = root;
    const accounts = ["dead", "renewable", "transient", "__main__"];
    writeFileSync(join(root, "config.json"), JSON.stringify({
      providers: { api: { adapter: "openai", baseUrl: "https://example.com/v1", apiKey: "fixture" } },
      codexAccounts: accounts.map(id => ({ id, email: `${id}@example.com`, isMain: id === "__main__" })),
    }));
    writeFileSync(join(root, "codex-accounts.json"), JSON.stringify({
      dead: { lastCodexValidationStatus: "failed", lastCodexValidationTerminal: true },
      renewable: { expiresAt: 1 },
      transient: { lastCodexValidationStatus: "failed", lastCodexValidationTerminal: false },
      __main__: { lastCodexValidationStatus: "failed", lastCodexValidationTerminal: true },
    }));
    const source = join(root, "accounts.json");
    writeFileSync(source, "source fixture");
    const other = join(root, "other");
    mkdirSync(other);
    writeFileSync(join(other, "config.json"), "other fixture");
    expect(await cleanupExpiredPoolAccounts()).toEqual({ removedCount: 1 });
    const config = JSON.parse(readFileSync(join(root, "config.json"), "utf8"));
    expect(config.codexAccounts.map((account: { id: string }) => account.id)).toEqual(accounts.slice(1));
    expect(config.providers.api.apiKey).toBe("fixture");
    expect(readFileSync(source, "utf8")).toBe("source fixture");
    expect(readFileSync(join(other, "config.json"), "utf8")).toBe("other fixture");
    writeFileSync(join(root, "codex-accounts.json"), "invalid json");
    const before = readFileSync(join(root, "config.json"), "utf8");
    await expect(cleanupExpiredPoolAccounts()).rejects.toThrow();
    expect(readFileSync(join(root, "config.json"), "utf8")).toBe(before);
  });
  test("binds API keys to isolated providers and deletes only the copy", async () => {
    const root = temporaryRoot();
    const target = join(root, "target");
    mkdirSync(target);
    process.env.OPENCODEX_HOME = target;
    const source = join(root, "accounts.json");
    writeFileSync(source, JSON.stringify({ accounts: [{ id: "api-one", account_name: "Test API", openai_api_key: "secret-fixture", api_base_url: "https://example.com/v1", default_model: "test-model" }] }));
    writeFileSync(join(target, "config.json"), JSON.stringify({ defaultProvider: "ollama", providers: { ollama: { adapter: "openai", baseUrl: "http://localhost:11434/v1", authMode: "local" } } }));
    const result = importSwitcherAccounts(["api-one"], source);
    expect(result.importedCount).toBe(1);
    expect(JSON.stringify(result)).not.toContain("secret-fixture");
    const id = result.imported[0]!.targetAccountId;
    const saved = JSON.parse(readFileSync(join(target, "config.json"), "utf8"));
    expect(saved.providers[id].apiKey).toBe("secret-fixture");
    expect(saved.providers[id].models).toEqual(["test-model"]);
    expect(saved.defaultProvider).toBe("ollama");
    expect(scanSwitcherAccounts(source, target).accounts[0]!.deletable).toBe(true);
    expect(scanSwitcherAccounts(source, join(root, "other")).eligibleCount).toBe(1);
    expect(importSwitcherAccounts(["api-one"], source).importedCount).toBe(0);
    const configPath = join(target, "config.json");
    const original = readFileSync(configPath, "utf8");
    const modified = JSON.parse(original);
    modified.providers[id].apiKey = "replacement-secret";
    writeFileSync(configPath, JSON.stringify(modified));
    expect(scanSwitcherAccounts(source, target).accounts[0]!.deletable).toBe(false);
    await expect(deleteSwitcherAccountForCurrentRuntime("api-one", source, { findLiveProxy: async () => null })).rejects.toThrow("来源不匹配");
    writeFileSync(configPath, original);
    const referenced = JSON.parse(original);
    referenced.defaultProvider = id;
    writeFileSync(configPath, JSON.stringify(referenced));
    await expect(deleteSwitcherAccountForCurrentRuntime("api-one", source, { findLiveProxy: async () => null })).rejects.toThrow("仍被配置引用");
    writeFileSync(configPath, original);
    await deleteSwitcherAccountForCurrentRuntime("api-one", source, { findLiveProxy: async () => null });
    expect(scanSwitcherAccounts(source, target).eligibleCount).toBe(1);
    expect(JSON.parse(readFileSync(source, "utf8")).accounts).toHaveLength(1);
  });
  test("does not offer hidden accounts for import", () => {
    const root = temporaryRoot();
    const sourcePath = join(root, "accounts.json");
    mkdirSync(root, { recursive: true });
    const accessToken = jwt({
      email: "hidden@example.com",
      "https://api.openai.com/auth": { chatgpt_account_id: "hidden-chatgpt-account" },
    });
    writeFileSync(sourcePath, JSON.stringify({
      accounts: [{
        id: "hidden-one",
        email: "hidden@example.com",
        is_hidden: true,
        tokens: { access_token: accessToken, refresh_token: "refresh-secret" },
      }],
    }));
    const scan = scanSwitcherAccounts(sourcePath, join(root, "target"));
    expect(scan.totalCount).toBe(1);
    expect(scan.eligibleCount).toBe(0);
    expect(scan.accounts[0]?.status).toBe("unsupported");
    expect(scan.accounts[0]?.reason).toContain("隐身");
  });

  test("does not allow deleting a different OpenCodex account with the same ChatGPT identity", () => {
    const root = temporaryRoot();
    const sourcePath = join(root, "accounts.json");
    const targetDir = join(root, "target");
    mkdirSync(targetDir, { recursive: true });
    const accessToken = jwt({
      email: "member@example.com",
      "https://api.openai.com/auth": { chatgpt_account_id: "shared-chatgpt-account" },
    });
    writeFileSync(sourcePath, JSON.stringify({
      accounts: [{
        id: "oauth-source",
        email: "member@example.com",
        tokens: { access_token: accessToken, refresh_token: "refresh-secret" },
      }],
    }));
    writeFileSync(join(targetDir, "config.json"), JSON.stringify({
      codexAccounts: [{ id: "manual-account", email: "member@example.com", isMain: false }],
    }));
    writeFileSync(join(targetDir, "codex-accounts.json"), JSON.stringify({
      "manual-account": {
        credential: { chatgptAccountId: "shared-chatgpt-account" },
      },
    }));

    const scan = scanSwitcherAccounts(sourcePath, targetDir);
    expect(scan.accounts[0]?.status).toBe("already_imported");
    expect(scan.accounts[0]?.deletable).toBe(false);
  });

  test("shows only redacted summaries and imports selected renewable OAuth accounts", async () => {
    const root = temporaryRoot();
    const sourceDir = join(root, "source");
    const targetDir = join(root, "target");
    mkdirSync(sourceDir, { recursive: true });
    mkdirSync(targetDir, { recursive: true });
    process.env.OPENCODEX_HOME = targetDir;

    const accessToken = jwt({
      email: "member@example.com",
      exp: 2_000_000_000,
      "https://api.openai.com/auth": { chatgpt_account_id: "chatgpt-account-one" },
    });
    const sourcePath = join(sourceDir, "accounts.json");
    writeFileSync(sourcePath, JSON.stringify({
      current_account_id: "oauth-one",
      accounts: [
        {
          id: "oauth-one",
          email: "member@example.com",
          plan_type: "free",
          access_token_expires_at: "2033-05-18T03:33:20Z",
          tokens: { access_token: accessToken, refresh_token: "refresh-secret-one" },
        },
        {
          id: "api-key-one",
          email: "key@example.com",
          auth_mode: "apikey",
          openai_api_key: "sk-test-not-a-real-secret",
          tokens: {},
        },
      ],
    }));
    writeFileSync(targetDir + "/config.json", JSON.stringify({
      port: 10100,
      defaultProvider: "ollama",
      providers: {
        ollama: { baseUrl: "http://127.0.0.1:11434/v1", authMode: "local", adapter: "openai" },
      },
    }));
    writeFileSync(targetDir + "/codex-accounts.json", "{}");

    const scan = scanSwitcherAccounts(sourcePath, targetDir);
    expect(scan.totalCount).toBe(2);
    expect(scan.eligibleCount).toBe(2);
    expect(scan.accounts[0]?.email).toBe("m***r@example.com");
    expect(JSON.stringify(scan)).not.toContain(accessToken);
    expect(JSON.stringify(scan)).not.toContain("refresh-secret-one");
    expect(scan.accounts[1]?.reason).toContain("API Key");

    const result = importSwitcherAccounts(["oauth-one"], sourcePath);
    expect(result.importedCount).toBe(1);
    expect(result.skippedCount).toBe(0);

    const config = JSON.parse(readFileSync(join(targetDir, "config.json"), "utf8"));
    const credentialStore = JSON.parse(readFileSync(join(targetDir, "codex-accounts.json"), "utf8"));
    expect(config.codexAccounts).toEqual(expect.arrayContaining([
      expect.objectContaining({
        id: "switcher-oauth-one",
        email: "member@example.com",
        plan: "free",
        isMain: false,
      }),
    ]));
    expect(config.codexSwitcherSources).toEqual({ "switcher-oauth-one": "oauth-one" });
    expect(credentialStore["switcher-oauth-one"]).toEqual(expect.objectContaining({
      generation: 1,
      credential: {
        accessToken,
        refreshToken: "refresh-secret-one",
        expiresAt: Date.parse("2033-05-18T03:33:20Z"),
        chatgptAccountId: "chatgpt-account-one",
      },
    }));

    const rescanned = scanSwitcherAccounts(sourcePath, targetDir);
    expect(rescanned.eligibleCount).toBe(1);
    expect(rescanned.accounts[0]?.status).toBe("already_imported");
    expect(rescanned.accounts[0]?.deletable).toBe(true);

    const runtimeRequests: Array<{ path: string; method?: string }> = [];
    await deleteSwitcherAccountForCurrentRuntime("oauth-one", sourcePath, {
      findLiveProxy: async () => ({ pid: 42, port: 15800, source: "runtime" }),
      runtimeRequest: async (path, init) => {
        runtimeRequests.push({ path, method: init.method });
        return { ok: true };
      },
    });
    expect(runtimeRequests).toEqual([{
      path: "/api/codex-auth/accounts?id=switcher-oauth-one",
      method: "DELETE",
    }]);

    const deleted = deleteSwitcherAccount("oauth-one", sourcePath);
    expect(deleted.targetAccountId).toBe("switcher-oauth-one");
    expect(deleted.deleted).toBe(true);
    expect(JSON.parse(readFileSync(sourcePath, "utf8")).accounts).toHaveLength(2);
    const configAfterDelete = JSON.parse(readFileSync(join(targetDir, "config.json"), "utf8"));
    expect(configAfterDelete.codexAccounts).not.toEqual(expect.arrayContaining([
      expect.objectContaining({ id: "switcher-oauth-one" }),
    ]));
    const credentialsAfterDelete = JSON.parse(
      readFileSync(join(targetDir, "codex-accounts.json"), "utf8"),
    );
    expect(credentialsAfterDelete["switcher-oauth-one"]?.credential).toBeUndefined();

    const scanAfterDelete = scanSwitcherAccounts(sourcePath, targetDir);
    expect(scanAfterDelete.accounts[0]?.status).toBe("ready");
    expect(scanAfterDelete.accounts[0]?.eligible).toBe(true);
    expect(scanAfterDelete.accounts[0]?.deletable).toBe(false);
    const deletedAgain = deleteSwitcherAccount("oauth-one", sourcePath);
    expect(deletedAgain.deleted).toBe(false);
  });

  test("does not mark a same-ID manual account as deletable", () => {
    const root = temporaryRoot();
    const sourcePath = join(root, "accounts.json");
    const targetDir = join(root, "target");
    mkdirSync(targetDir, { recursive: true });
    const accessToken = jwt({
      email: "manual@example.com",
      "https://api.openai.com/auth": { chatgpt_account_id: "manual-chatgpt" },
    });
    writeFileSync(sourcePath, JSON.stringify({
      accounts: [{
        id: "oauth-one",
        email: "manual@example.com",
        tokens: { access_token: accessToken, refresh_token: "refresh-secret" },
      }],
    }));
    writeFileSync(join(targetDir, "config.json"), JSON.stringify({
      codexAccounts: [{ id: "switcher-oauth-one", email: "manual@example.com", isMain: false }],
    }));
    writeFileSync(join(targetDir, "codex-accounts.json"), JSON.stringify({
      "switcher-oauth-one": { credential: { chatgptAccountId: "manual-chatgpt" } },
    }));

    const scan = scanSwitcherAccounts(sourcePath, targetDir);
    expect(scan.accounts[0]?.status).toBe("already_imported");
    expect(scan.accounts[0]?.deletable).toBe(false);
    expect(deleteSwitcherAccount("oauth-one", sourcePath)).toEqual(expect.objectContaining({
      deleted: false,
    }));
  });

  test("recognizes and deletes legacy Switcher imports after provenance migration is initialized", () => {
    const root = temporaryRoot();
    const sourcePath = join(root, "accounts.json");
    const targetDir = join(root, "target");
    mkdirSync(targetDir, { recursive: true });
    process.env.OPENCODEX_HOME = targetDir;
    const accessToken = jwt({
      email: "legacy@example.com",
      "https://api.openai.com/auth": { chatgpt_account_id: "legacy-chatgpt" },
    });
    writeFileSync(sourcePath, JSON.stringify({
      accounts: [{
        id: "legacy-one",
        email: "legacy@example.com",
        tokens: { access_token: accessToken, refresh_token: "refresh-secret" },
      }],
    }));
    writeFileSync(join(targetDir, "config.json"), JSON.stringify({
      codexAccounts: [{ id: "switcher-legacy-one", email: "legacy@example.com", isMain: false }],
      codexSwitcherSources: { "switcher-legacy-sibling": "legacy-sibling" },
    }));
    writeFileSync(join(targetDir, "codex-accounts.json"), JSON.stringify({
      "switcher-legacy-one": { credential: { chatgptAccountId: "legacy-chatgpt" } },
      "switcher-legacy-sibling": { credential: { chatgptAccountId: "legacy-sibling-chatgpt" } },
    }));

    const scan = scanSwitcherAccounts(sourcePath, targetDir);
    expect(scan.accounts[0]).toEqual(expect.objectContaining({
      status: "already_imported",
      deletable: true,
    }));
    expect(deleteSwitcherAccount("legacy-one", sourcePath)).toEqual(expect.objectContaining({
      deleted: true,
    }));
    expect(JSON.parse(readFileSync(join(targetDir, "config.json"), "utf8")).codexAccounts).toEqual([]);
  });

  test("legacy fallback still rejects a deterministic ID with the wrong identity", () => {
    const root = temporaryRoot();
    const sourcePath = join(root, "accounts.json");
    const targetDir = join(root, "target");
    mkdirSync(targetDir, { recursive: true });
    process.env.OPENCODEX_HOME = targetDir;
    const accessToken = jwt({
      email: "source@example.com",
      "https://api.openai.com/auth": { chatgpt_account_id: "source-chatgpt" },
    });
    writeFileSync(sourcePath, JSON.stringify({
      accounts: [{
        id: "legacy-one",
        email: "source@example.com",
        tokens: { access_token: accessToken, refresh_token: "refresh-secret" },
      }],
    }));
    writeFileSync(join(targetDir, "config.json"), JSON.stringify({
      codexAccounts: [{ id: "switcher-legacy-one", email: "manual@example.com", isMain: false }],
      codexSwitcherSources: {},
    }));
    writeFileSync(join(targetDir, "codex-accounts.json"), JSON.stringify({
      "switcher-legacy-one": { credential: { chatgptAccountId: "manual-chatgpt" } },
    }));

    const scan = scanSwitcherAccounts(sourcePath, targetDir);
    expect(scan.accounts[0]?.deletable).toBe(false);
    expect(deleteSwitcherAccount("legacy-one", sourcePath)).toEqual(expect.objectContaining({
      deleted: false,
      message: expect.stringContaining("身份不匹配"),
    }));
  });
});
