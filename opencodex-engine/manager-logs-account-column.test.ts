import { describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { applyAccountColumnPatch, patchGuiAccountColumn } from "./manager-logs-account-column";

const HEADER_OLD = "(0,Y.jsx)(`th`,{className:`log-col-model`,children:t(`logs.col.model`)}),(0,Y.jsx)(`th`,{children:t(`logs.col.effort`)})";
const COL_OLD = "(0,Y.jsx)(`col`,{className:`logs-col-model`}),(0,Y.jsx)(`col`,{className:`logs-col-effort`})";
const ROW_OLD = "(0,Y.jsx)(`td`,{className:`mono log-reasoning-cell`,title:i,children:pv(r)})";
const DETAIL_OLD = "(0,Y.jsx)(`span`,{className:`muted`,children:a(`logs.col.provider`)}),(0,Y.jsx)(`span`,{children:Ln(e.provider,a)}),";
const SPACER_OLD = "colSpan:10,className:`logs-virtual-spacer`";

function fakeBundle(): string {
  return [
    "import{a}from'b';",
    HEADER_OLD,
    COL_OLD,
    ROW_OLD,
    DETAIL_OLD,
    SPACER_OLD,
    SPACER_OLD,
    "export{}",
  ].join("\n");
}

test("account column patch inserts header, row, detail cells and widens spacers", () => {
  const patched = applyAccountColumnPatch(fakeBundle());
  expect(patched).toContain("children:`账号`}),(0,Y.jsx)(`th`,{children:t(`logs.col.effort`)})");
  expect(patched).toContain("(0,Y.jsx)(`col`,{style:{width:190}}),(0,Y.jsx)(`col`,{className:`logs-col-effort`})");
  expect(patched).toContain("log-col-account");
  expect(patched).toContain("window.__ocxAccLabel(e.provider)");
  expect(patched.split("colSpan:11,className:`logs-virtual-spacer`").length - 1).toBe(2);
  expect(patched).not.toContain(SPACER_OLD);
  expect(patched.split("whiteSpace:`pre-line`").length - 1).toBe(2);
  expect(patched).toContain("switcher-") && expect(patched).toContain("log-col-account");
  expect(patched.startsWith("(()=>{try{window.__ocxAccMap")).toBe(true);
});

test("account column patch is a no-op on already patched bundles", () => {
  const patched = applyAccountColumnPatch(fakeBundle());
  expect(applyAccountColumnPatch(patched)).toBe(patched);
});

test("account column patch fails loudly when anchors drift", () => {
  expect(() => applyAccountColumnPatch("export{}")).toThrow("锚点不匹配");
  expect(() => applyAccountColumnPatch(fakeBundle().replace(ROW_OLD, ""))).toThrow("日志行");
});

describe("patchGuiAccountColumn", () => {
  function makeEngineDir(): string {
    const root = mkdtempSync(join(tmpdir(), "ocx-engine-"));
    const assets = join(root, "node_modules", "@bitkyc08", "opencodex", "gui", "dist", "assets");
    mkdirSync(assets, { recursive: true });
    writeFileSync(join(assets, "index-deadbeef.js"), fakeBundle());
    return root;
  }

  test("patches the bundle once and keeps an original backup", () => {
    const engineDir = makeEngineDir();
    const bundle = join(engineDir, "node_modules", "@bitkyc08", "opencodex", "gui", "dist", "assets", "index-deadbeef.js");
    const first = patchGuiAccountColumn(engineDir);
    expect(first.patched).toBe(true);
    expect(readFileSync(bundle, "utf8")).toContain("__ocxAccMap");
    expect(readFileSync(`${bundle}.orig-bak`, "utf8")).toBe(fakeBundle());

    const second = patchGuiAccountColumn(engineDir);
    expect(second.patched).toBe(false);
    expect(second.reason).toContain("已包含账号列补丁");
  });

  test("missing gui assets are reported instead of throwing", () => {
    const root = mkdtempSync(join(tmpdir(), "ocx-engine-empty-"));
    expect(patchGuiAccountColumn(root)).toEqual({ patched: false, reason: "缺少 Engine GUI 资源目录" });
  });

  test("multiple bundles are refused", () => {
    const engineDir = makeEngineDir();
    const assets = join(engineDir, "node_modules", "@bitkyc08", "opencodex", "gui", "dist", "assets");
    writeFileSync(join(assets, "index-cafebabe.js"), fakeBundle());
    const result = patchGuiAccountColumn(engineDir);
    expect(result.patched).toBe(false);
    expect(result.reason).toContain("不唯一");
    expect(readdirSync(assets).filter(name => name.endsWith(".js")).length).toBe(2);
    expect(existsSync(join(assets, "index-deadbeef.js.orig-bak"))).toBe(false);
  });
});
