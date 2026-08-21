import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { importSwitcherAccounts, scanSwitcherAccounts } from "./manager-switcher-import.ts";

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

  test("shows only redacted summaries and imports selected renewable OAuth accounts", () => {
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
    expect(scan.eligibleCount).toBe(1);
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
    expect(rescanned.eligibleCount).toBe(0);
    expect(rescanned.accounts[0]?.status).toBe("already_imported");
  });
});
