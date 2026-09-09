import { existsSync } from "node:fs";
import { isAbsolute, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

/** Only the backend-selected, downloaded Engine may supply implementation modules. */
export function resolveOpenCodexPackageRoot(configuredRoot = process.env.OPENCODEX_PACKAGE_ROOT): string {
  const requested = configuredRoot?.trim();
  if (!requested || !isAbsolute(requested)) {
    throw new Error("尚未安装或选择 OpenCodex Engine，请先在版本管理中下载并激活版本");
  }
  const root = resolve(requested);
  if (!existsSync(join(root, "src", "config.ts"))) {
    throw new Error("所选 OpenCodex Engine 文件缺失，请重新下载该版本");
  }
  return root;
}

export function importEngineModule(relativePath: string) {
  return import(pathToFileURL(join(resolveOpenCodexPackageRoot(), relativePath)).href);
}
