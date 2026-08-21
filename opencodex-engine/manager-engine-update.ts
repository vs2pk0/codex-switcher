import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const REPOSITORY = "lidge-jun/opencodex";
const PACKAGE_NAME = "@bitkyc08/opencodex";
const GITHUB_API = `https://api.github.com/repos/${REPOSITORY}`;
const REGISTRY_API = "https://registry.npmjs.org/@bitkyc08%2Fopencodex";
const REGISTRY_ORIGIN = "https://registry.npmjs.org/";
const REQUEST_TIMEOUT_MS = 15_000;
const MAX_RELEASES = 30;

export interface EngineRelease {
  version: string;
  tag: string;
  name: string;
  prerelease: boolean;
  publishedAt: string;
  url: string;
}

interface GitHubRelease {
  tag_name?: unknown;
  name?: unknown;
  draft?: unknown;
  prerelease?: unknown;
  published_at?: unknown;
  html_url?: unknown;
}

interface InstallRequest {
  version?: unknown;
  engineRoot?: unknown;
}

function requiredText(value: unknown, label: string): string {
  if (typeof value !== "string" || !value.trim()) throw new Error(`${label}无效`);
  return value.trim();
}

export function normalizeReleaseVersion(tag: unknown): string | null {
  if (typeof tag !== "string") return null;
  const value = tag.startsWith("v") ? tag.slice(1) : tag;
  return /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(value) ? value : null;
}

export function parseReleases(value: unknown): EngineRelease[] {
  if (!Array.isArray(value)) throw new Error("GitHub Releases 返回格式无效");
  const releases: EngineRelease[] = [];
  for (const raw of value.slice(0, MAX_RELEASES)) {
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) continue;
    const release = raw as GitHubRelease;
    if (release.draft === true) continue;
    const version = normalizeReleaseVersion(release.tag_name);
    if (!version) continue;
    const tag = requiredText(release.tag_name, "Release 标签");
    const url = requiredText(release.html_url, "Release 地址");
    if (!url.startsWith(`https://github.com/${REPOSITORY}/releases/`)) continue;
    releases.push({
      version,
      tag,
      name: typeof release.name === "string" && release.name.trim() ? release.name.trim() : tag,
      prerelease: release.prerelease === true,
      publishedAt: typeof release.published_at === "string" ? release.published_at : "",
      url,
    });
  }
  if (releases.length === 0) throw new Error("GitHub Releases 中没有可用的 OpenCodex 版本");
  return releases;
}

async function fetchJson(url: string): Promise<unknown> {
  const response = await fetch(url, {
    headers: {
      Accept: "application/vnd.github+json, application/json",
      "User-Agent": "OpenCodex-Manager",
      "X-GitHub-Api-Version": "2022-11-28",
    },
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  });
  if (!response.ok) throw new Error(`版本服务请求失败（HTTP ${response.status}）`);
  return await response.json();
}

export async function fetchReleaseCatalog(): Promise<{ releases: EngineRelease[] }> {
  return { releases: parseReleases(await fetchJson(`${GITHUB_API}/releases?per_page=${MAX_RELEASES}`)) };
}

function validateInstalledPackage(directory: string, expectedVersion: string): void {
  const packageRoot = join(directory, "node_modules", "@bitkyc08", "opencodex");
  const manifest = JSON.parse(readFileSync(join(packageRoot, "package.json"), "utf8")) as { version?: unknown };
  if (manifest.version !== expectedVersion) throw new Error("下载后的 Engine 版本与目标版本不一致");
  if (!existsSync(join(packageRoot, "src", "cli", "index.ts"))) throw new Error("下载后的 Engine 缺少 CLI 入口");
}

async function installVersion(request: InstallRequest): Promise<{ version: string; integrity: string; releaseUrl: string }> {
  const version = normalizeReleaseVersion(requiredText(request.version, "版本号"));
  if (!version || version !== request.version) throw new Error("版本号格式无效");
  const engineRoot = requiredText(request.engineRoot, "Engine 目录");
  mkdirSync(engineRoot, { recursive: true });

  const releaseValue = await fetchJson(`${GITHUB_API}/releases/tags/v${encodeURIComponent(version)}`);
  const releases = parseReleases([releaseValue]);
  const release = releases.find(item => item.version === version);
  if (!release) throw new Error(`GitHub Releases 中不存在 v${version}`);

  const registry = await fetchJson(`${REGISTRY_API}/${encodeURIComponent(version)}`) as {
    name?: unknown;
    version?: unknown;
    dist?: { integrity?: unknown; tarball?: unknown };
  };
  if (registry.name !== PACKAGE_NAME || registry.version !== version) {
    throw new Error("npm Registry 包身份或版本不匹配");
  }
  const integrity = requiredText(registry.dist?.integrity, "包完整性信息");
  const tarball = requiredText(registry.dist?.tarball, "包下载地址");
  if (!integrity.startsWith("sha512-") || !tarball.startsWith(`${REGISTRY_ORIGIN}@bitkyc08/opencodex/-/`)) {
    throw new Error("npm Registry 返回了不受信任的包信息");
  }

  const finalDir = join(engineRoot, version);
  if (existsSync(finalDir)) {
    validateInstalledPackage(finalDir, version);
    return { version, integrity, releaseUrl: release.url };
  }

  const tempDir = join(engineRoot, `.install-${version}-${crypto.randomUUID()}`);
  mkdirSync(tempDir, { recursive: true });
  try {
    writeFileSync(join(tempDir, "package.json"), JSON.stringify({
      name: "opencodex-manager-managed-engine",
      private: true,
      version,
      dependencies: { [PACKAGE_NAME]: version },
    }, null, 2));
    const child = Bun.spawn([
      process.execPath,
      "install",
      "--production",
      "--exact",
      "--ignore-scripts",
      "--no-progress",
      "--backend=copyfile",
      `--registry=${REGISTRY_ORIGIN}`,
      `--cwd=${tempDir}`,
    ], {
      cwd: tempDir,
      env: { ...process.env, NO_COLOR: "1", FORCE_COLOR: "0" },
      stdout: "pipe",
      stderr: "pipe",
    });
    const [exitCode, stderr] = await Promise.all([
      child.exited,
      new Response(child.stderr).text(),
    ]);
    if (exitCode !== 0) {
      const safeDetail = stderr.replace(/https?:\/\/[^\s]+/g, "[URL]").trim().slice(-1200);
      throw new Error(safeDetail ? `Engine 下载失败：${safeDetail}` : "Engine 下载失败");
    }
    validateInstalledPackage(tempDir, version);
    writeFileSync(join(tempDir, ".install.json"), JSON.stringify({
      version,
      integrity,
      releaseUrl: release.url,
      installedAt: new Date().toISOString(),
    }, null, 2));
    renameSync(tempDir, finalDir);
    return { version, integrity, releaseUrl: release.url };
  } catch (error) {
    rmSync(tempDir, { recursive: true, force: true });
    throw error;
  }
}

async function readStdin(): Promise<string> {
  return await new Response(Bun.stdin.stream()).text();
}

async function main() {
  const command = process.argv[2];
  if (command === "catalog") {
    console.log(JSON.stringify(await fetchReleaseCatalog()));
    return;
  }
  if (command === "install") {
    console.log(JSON.stringify(await installVersion(JSON.parse(await readStdin()) as InstallRequest)));
    return;
  }
  throw new Error("不支持的 Engine 更新操作");
}

if (import.meta.main) {
  main().catch(error => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
