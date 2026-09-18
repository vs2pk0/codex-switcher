import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const GUI_ASSETS_RELATIVE = join("node_modules", "@bitkyc08", "opencodex", "gui", "dist", "assets");
const PATCH_MARKER = "__ocxAccMap";

const BOOTSTRAP = "(()=>{try{window.__ocxAccMap={};window.__ocxAccLabel=function(pr){const p=String(pr||'');const lbl=p.indexOf('openai-')===0?p.slice(7):p;const e=(window.__ocxAccMap||{})[lbl];if(!e)return lbl||'';let id=e.id||'';if(id.indexOf('switcher-')===0)id=id.slice(9);return e.email+(e.plan?' · '+e.plan:'')+(id?'\\n'+id:'');};(async()=>{for(let k=0;k<12;k++){try{const r=await fetch('/api/codex-auth/accounts');if(r.ok){const d=await r.json();const list=Array.isArray(d)?d:((d&&d.accounts)||[]);const map={};for(const a of list){if(a&&a.logLabel)map[a.logLabel]={email:a.email||'',plan:a.plan||'',id:a.id||''};}window.__ocxAccMap=map;window.dispatchEvent(new Event('resize'));return;}}catch(e){}await new Promise(res=>setTimeout(res,400+k*400));}})();}catch(e){}})();\n";

const HEADER_OLD = "(0,Y.jsx)(`th`,{className:`log-col-model`,children:t(`logs.col.model`)}),(0,Y.jsx)(`th`,{children:t(`logs.col.effort`)})";
const HEADER_NEW = "(0,Y.jsx)(`th`,{className:`log-col-model`,children:t(`logs.col.model`)}),(0,Y.jsx)(`th`,{children:`账号`}),(0,Y.jsx)(`th`,{children:t(`logs.col.effort`)})";
const ROW_OLD = "(0,Y.jsx)(`td`,{className:`mono log-reasoning-cell`,title:i,children:pv(r)})";
const ROW_NEW = "(0,Y.jsx)(`td`,{className:`muted mono log-col-account`,style:{whiteSpace:`pre-line`},children:window.__ocxAccLabel?window.__ocxAccLabel(r.provider):''})," + ROW_OLD;
const COL_OLD = "(0,Y.jsx)(`col`,{className:`logs-col-model`}),(0,Y.jsx)(`col`,{className:`logs-col-effort`})";
const COL_NEW = "(0,Y.jsx)(`col`,{className:`logs-col-model`}),(0,Y.jsx)(`col`,{style:{width:190}}),(0,Y.jsx)(`col`,{className:`logs-col-effort`})";
const DETAIL_OLD = "(0,Y.jsx)(`span`,{className:`muted`,children:a(`logs.col.provider`)}),(0,Y.jsx)(`span`,{children:Ln(e.provider,a)}),";
const DETAIL_NEW = DETAIL_OLD + "(0,Y.jsx)(`span`,{className:`muted`,children:`账号`}),(0,Y.jsx)(`span`,{className:`mono`,style:{whiteSpace:`pre-line`},children:window.__ocxAccLabel?window.__ocxAccLabel(e.provider):''}),";
const SPACER_OLD = "colSpan:10,className:`logs-virtual-spacer`";
const SPACER_NEW = "colSpan:11,className:`logs-virtual-spacer`";

function replaceExactly(source: string, search: string, replacement: string, expected: number, label: string): string {
  const found = source.split(search).length - 1;
  if (found !== expected) {
    throw new Error(`日志账号列补丁锚点不匹配：${label} 期望 ${expected} 处，实际 ${found} 处（Engine 版本变化？）`);
  }
  return source.split(search).join(replacement);
}

export function applyAccountColumnPatch(source: string): string {
  if (source.includes(PATCH_MARKER)) return source;
  let patched = replaceExactly(source, HEADER_OLD, HEADER_NEW, 1, "日志表头");
  patched = replaceExactly(patched, COL_OLD, COL_NEW, 1, "日志列宽");
  patched = replaceExactly(patched, ROW_OLD, ROW_NEW, 1, "日志行");
  patched = replaceExactly(patched, DETAIL_OLD, DETAIL_NEW, 1, "请求详情");
  patched = replaceExactly(patched, SPACER_OLD, SPACER_NEW, 2, "虚拟滚动占位");
  return BOOTSTRAP + patched;
}

export interface GuiPatchResult {
  patched: boolean;
  bundle?: string;
  reason?: string;
}

export function patchGuiAccountColumn(engineDir: string): GuiPatchResult {
  const assetsDir = join(engineDir, GUI_ASSETS_RELATIVE);
  if (!existsSync(assetsDir)) return { patched: false, reason: "缺少 Engine GUI 资源目录" };
  const bundles = readdirSync(assetsDir).filter(name => name.startsWith("index-") && name.endsWith(".js"));
  if (bundles.length === 0) return { patched: false, reason: "未找到 Engine GUI bundle" };
  if (bundles.length > 1) return { patched: false, reason: `Engine GUI bundle 不唯一：${bundles.join(", ")}` };
  const bundle = join(assetsDir, bundles[0]);
  const source = readFileSync(bundle, "utf8");
  if (source.includes(PATCH_MARKER)) return { patched: false, bundle, reason: "该 bundle 已包含账号列补丁" };
  const backup = `${bundle}.orig-bak`;
  if (!existsSync(backup)) writeFileSync(backup, source);
  writeFileSync(bundle, applyAccountColumnPatch(source));
  return { patched: true, bundle };
}

export function defaultManagedEngineDir(): string {
  const managerRoot = join(homedir(), "Library", "Application Support", "com.codex.switcher", "opencodex-manager");
  const active = JSON.parse(readFileSync(join(managerRoot, "active-engine.json"), "utf8")) as { version?: unknown };
  if (typeof active.version !== "string" || !active.version.trim()) throw new Error("active-engine.json 缺少版本号");
  return join(managerRoot, "engines", active.version.trim());
}

async function main() {
  const engineDir = process.argv[2] ? process.argv[2] : defaultManagedEngineDir();
  console.log(JSON.stringify({ engineDir, ...patchGuiAccountColumn(engineDir) }));
}

if (import.meta.main) {
  main().catch(error => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
