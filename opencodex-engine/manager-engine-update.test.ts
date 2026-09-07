import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { installVersion, normalizeReleaseVersion, parseReleases, readVerifiedPackage, type EngineProgress } from "./manager-engine-update";

const packageBytes = new TextEncoder().encode("verified engine package");
const integrity = "sha512-" + createHash("sha512").update(packageBytes).digest("base64");

test("package download reports actual bytes and verifies integrity", async () => {
  const progress: EngineProgress[] = [];
  const response = new Response(new ReadableStream({
    start(controller) {
      controller.enqueue(packageBytes.slice(0, 5));
      controller.enqueue(packageBytes.slice(5));
      controller.close();
    },
  }), { headers: { "content-length": String(packageBytes.length) } });
  expect(await readVerifiedPackage(response, integrity, (event) => progress.push(event))).toEqual(Buffer.from(packageBytes));
  expect(progress[0]).toEqual({ stage: "downloading", downloadedBytes: 0, totalBytes: packageBytes.length });
  expect(progress.at(-1)).toEqual({ stage: "downloading", downloadedBytes: packageBytes.length, totalBytes: packageBytes.length });
  expect(progress.some((event) => event.downloadedBytes === 5)).toBe(true);
});

test("unknown download size stays indeterminate until all bytes are verified", async () => {
  const progress: EngineProgress[] = [];
  await readVerifiedPackage(new Response(packageBytes), integrity, (event) => progress.push(event));
  expect(progress[0].totalBytes).toBeUndefined();
  expect(progress.at(-1)?.totalBytes).toBe(packageBytes.length);
});

test("failed, truncated, tampered and oversized packages cannot be installed", async () => {
  await expect(readVerifiedPackage(new Response(null, { status: 503 }), integrity, () => {})).rejects.toThrow("HTTP 503");
  await expect(readVerifiedPackage(new Response(packageBytes, { headers: { "content-length": "100" } }), integrity, () => {})).rejects.toThrow("下载不完整");
  await expect(readVerifiedPackage(new Response(packageBytes), "sha512-invalid", () => {})).rejects.toThrow("完整性校验失败");
  await expect(readVerifiedPackage(new Response(packageBytes, { headers: { "content-length": String(257 * 1024 * 1024) } }), integrity, () => {})).rejects.toThrow("大小限制");
});

test("verified local tarballs install without lifecycle scripts and invalid packages never publish", async () => {
  const root = mkdtempSync(join(tmpdir(), "opencodex-engine-install-test-"));
  const originalFetch = globalThis.fetch;
  try {
    for (const valid of [true, false]) {
      const version = valid ? "1.0.0" : "1.0.1";
      const files: Record<string, string> = {
        "package/package.json": JSON.stringify({ name: "@bitkyc08/opencodex", version, scripts: { postinstall: "exit 42" } }),
      };
      if (valid) files["package/src/cli/index.ts"] = "export {};\n";
      const bytes = await new Bun.Archive(files, { compress: "gzip" }).bytes();
      const packageIntegrity = "sha512-" + createHash("sha512").update(bytes).digest("base64");
      const releaseUrl = `https://github.com/lidge-jun/opencodex/releases/tag/v${version}`;
      const tarball = `https://registry.npmjs.org/@bitkyc08/opencodex/-/opencodex-${version}.tgz`;
      globalThis.fetch = (async (input: string | URL | Request) => {
        const url = String(input);
        if (url === `https://api.github.com/repos/lidge-jun/opencodex/releases/tags/v${version}`) return Response.json({ tag_name: `v${version}`, html_url: releaseUrl });
        if (url === `https://registry.npmjs.org/@bitkyc08%2Fopencodex/${version}`) return Response.json({ name: "@bitkyc08/opencodex", version, dist: { integrity: packageIntegrity, tarball } });
        if (url === tarball) return new Response(bytes, { headers: { "content-length": String(bytes.length) } });
        throw new Error(`Unexpected network request: ${url}`);
      }) as typeof fetch;
      if (valid) {
        expect(await installVersion({ version, engineRoot: root })).toEqual({ version, integrity: packageIntegrity, releaseUrl });
        const installed = join(root, version, "node_modules", "@bitkyc08", "opencodex");
        expect(JSON.parse(readFileSync(join(installed, "package.json"), "utf8")).version).toBe(version);
        expect(existsSync(join(installed, "src", "cli", "index.ts"))).toBe(true);
      } else {
        await expect(installVersion({ version, engineRoot: root })).rejects.toThrow("缺少 CLI 入口");
        expect(existsSync(join(root, version))).toBe(false);
      }
    }
    expect(readdirSync(root)).toEqual(["1.0.0"]);
  } finally {
    globalThis.fetch = originalFetch;
    rmSync(root, { recursive: true, force: true });
  }
}, 20_000);

describe("OpenCodex release catalog", () => {
  test("accepts published semantic versions and excludes drafts or untrusted URLs", () => {
    expect(parseReleases([
      {
        tag_name: "v2.28.0",
        name: "v2.28.0",
        draft: false,
        prerelease: false,
        published_at: "2026-08-21T00:00:00Z",
        html_url: "https://github.com/lidge-jun/opencodex/releases/tag/v2.28.0",
      },
      {
        tag_name: "v2.29.0",
        draft: true,
        prerelease: false,
        html_url: "https://github.com/lidge-jun/opencodex/releases/tag/v2.29.0",
      },
      {
        tag_name: "v2.30.0",
        draft: false,
        prerelease: false,
        html_url: "https://example.com/v2.30.0",
      },
    ])).toEqual([{
      version: "2.28.0",
      tag: "v2.28.0",
      name: "v2.28.0",
      prerelease: false,
      publishedAt: "2026-08-21T00:00:00Z",
      url: "https://github.com/lidge-jun/opencodex/releases/tag/v2.28.0",
    }]);
  });

  test("rejects paths, latest aliases and malformed versions", () => {
    expect(normalizeReleaseVersion("v2.28.0-preview.20260821")).toBe("2.28.0-preview.20260821");
    expect(normalizeReleaseVersion("latest")).toBeNull();
    expect(normalizeReleaseVersion("v2.28.0/../../bad")).toBeNull();
    expect(normalizeReleaseVersion("v2.28")).toBeNull();
  });
});
