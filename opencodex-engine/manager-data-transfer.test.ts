import { afterEach, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { recoverTransfer, transferData, type TransferInput } from "./manager-data-transfer.ts";
const roots: string[] = [];
afterEach(() => { for (const path of roots.splice(0)) rmSync(path,{recursive:true,force:true}); });
function fixture(): TransferInput {
  const root = realpathSync(mkdtempSync(join(tmpdir(),"ocx-transfer-test-"))); roots.push(root);
  const source = join(root,"source"), target = join(root,"target"), manager = join(root,"manager");
  for (const p of [source,target,manager]) mkdirSync(p);
  const write = (dir: string, name: string, data: unknown) => writeFileSync(join(dir,name),JSON.stringify(data));
  write(source,"config.json",{port:15800,providers:{same:{baseUrl:"source"},new:{baseUrl:"new"}},codexAccounts:[{id:"same",email:"source"},{id:"new"}],clientIntegrations:{codex:true}});
  write(target,"config.json",{port:15801,providers:{same:{baseUrl:"target"}},codexAccounts:[{id:"same",email:"target"}],clientIntegrations:{codex:false}});
  write(source,"codex-accounts.json",{same:{credential:{refreshToken:"source"}},new:{credential:{refreshToken:"new"}}});
  write(target,"codex-accounts.json",{same:{credential:{refreshToken:"target"}}});
  writeFileSync(join(source,"admin-api-token"),"source-identity");
  writeFileSync(join(target,"admin-api-token"),"target-identity");
  writeFileSync(join(source,"usage.jsonl"),"source-history");
  writeFileSync(join(target,"usage.jsonl"),"target-history");
  return { source,target,manager,port:15801,mode:"merge",history:false,execute:false };
}
function execute(input: TransferInput) {
  const preview = transferData(input);
  return transferData({...input,execute:true,fingerprint:preview.fingerprint});
}
test("合并同时保留目标账号元数据和凭证，源数据不变", () => {
  const input = fixture(), before = readFileSync(join(input.source,"config.json"),"utf8");
  const result = execute(input);
  const config = JSON.parse(readFileSync(join(input.target,"config.json"),"utf8"));
  const credentials = JSON.parse(readFileSync(join(input.target,"codex-accounts.json"),"utf8"));
  expect(config.providers.same.baseUrl).toBe("target"); expect(config.providers.new.baseUrl).toBe("new");
  expect(config.codexAccounts[0].email).toBe("target");
  expect(credentials.same.credential.refreshToken).toBe("target"); expect(credentials.new.credential.refreshToken).toBe("new");
  expect(config.port).toBe(15801); expect(config.clientIntegrations.codex).toBe(false);
  expect(readFileSync(join(input.target,"admin-api-token"),"utf8")).toBe("target-identity");
  expect(readFileSync(join(input.source,"config.json"),"utf8")).toBe(before);
  expect(readFileSync(join(result.backupPath!,"usage.jsonl"),"utf8")).toBe("target-history");
});
test("覆盖配置及历史时仍保留目标身份，备份可恢复", () => {
  const input = {...fixture(),mode:"overwrite" as const,history:true};
  const result = execute(input);
  expect(readFileSync(join(input.target,"usage.jsonl"),"utf8")).toBe("source-history");
  expect(readFileSync(join(input.target,"admin-api-token"),"utf8")).toBe("target-identity");
  const config = JSON.parse(readFileSync(join(input.target,"config.json"),"utf8"));
  expect(config.providers.same.baseUrl).toBe("source"); expect(config.port).toBe(15801);
  expect(JSON.parse(readFileSync(join(result.backupPath!,"config.json"),"utf8")).providers.same.baseUrl).toBe("target");
});
test("拒绝过期预览、重叠目录和符号链接", () => {
  const input = fixture(), preview = transferData(input);
  writeFileSync(join(input.target,"config.json"),'{"port":15801}');
  expect(() => transferData({...input,execute:true,fingerprint:preview.fingerprint})).toThrow("重新预览");
  expect(() => transferData({...input,target:input.source})).toThrow("重叠");
  symlinkSync(input.source,join(input.manager,"link"));
  expect(() => transferData({...input,source:join(input.manager,"link")})).toThrow("符号链接");
});
test("切换目录后异常中断，下次恢复原目标", () => {
  const input = fixture();
  const backup = join(input.manager,"backup");
  renameSync(input.target,backup); mkdirSync(input.target);
  writeFileSync(join(input.target,"broken"),"incomplete");
  writeFileSync(join(input.manager,"transfer-journal.json"),JSON.stringify({target:input.target,backup,stage:join(roots.at(-1)!,"stage"),phase:"prepared"}));
  recoverTransfer(input.manager,input.target);
  expect(readFileSync(join(input.target,"usage.jsonl"),"utf8")).toBe("target-history");
});
test("不复制外部存储路径，预览不落盘", () => {
  const input = fixture();
  writeFileSync(join(input.source,"config.json"),JSON.stringify({storageDir:"/external/shared"}));
  expect(() => transferData(input)).toThrow("外部绝对目录");
});

test("Engine 版本变化会使预览失效", () => {
  const input = {...fixture(),sourceEngine:"2.45.0",targetEngine:null};
  const preview = transferData(input);
  expect(() => transferData({...input,sourceEngine:"2.48.0",execute:true,fingerprint:preview.fingerprint})).toThrow("重新预览");
});

test("未安装目标复制存储数据，不复制源管理令牌和运行身份", () => {
  const input = {...fixture(),sourceEngine:"2.45.0",targetEngine:null};
  rmSync(input.target,{recursive:true});
  mkdirSync(join(input.source,"artifacts"));
  writeFileSync(join(input.source,"artifacts/image.png"),"fixture-image");
  writeFileSync(join(input.source,"runtime-port.json"),'{"pid":123}');
  execute(input);
  expect(readFileSync(join(input.target,"artifacts/image.png"),"utf8")).toBe("fixture-image");
  expect(existsSync(join(input.target,"admin-api-token"))).toBe(false);
  expect(existsSync(join(input.target,"runtime-port.json"))).toBe(false);
  expect(JSON.parse(readFileSync(join(input.target,"config.json"),"utf8")).port).toBe(15801);
});

test("目标已有其他 Engine 但尚未初始化时仍复制便携数据", () => {
  const input = {...fixture(),sourceEngine:"2.45.0",targetEngine:"2.48.0"};
  rmSync(input.target,{recursive:true});
  mkdirSync(join(input.source,"artifacts"));
  writeFileSync(join(input.source,"artifacts/image.png"),"fixture-image");
  execute(input);
  expect(readFileSync(join(input.target,"artifacts/image.png"),"utf8")).toBe("fixture-image");
});
