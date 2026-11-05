import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import test from "node:test";
import ts from "typescript";

const source = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");
const handlers = source.slice(source.indexOf("async function startAppUpdateDownload("), source.indexOf("\nfunction openAppUpdateReleases("));
const compiled = ts.transpileModule(handlers, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;

function fixture(target = "macos-aarch64") {
  const windows = target.startsWith("windows-");
  const assetName = windows ? "update.exe" : "update.dmg";
  const path = windows ? "C:\\Users\\测试 用户\\Downloads\\Codex Switcher\\updates\\1.2.3\\update.exe" : "/updates/1.2.3/update.dmg";
  const info = { target, latestVersion: "1.2.3", assetName, assetSize: 10 };
  const refs = {
    appUpdateInfo: { value: info },
    appUpdateProgress: { value: null },
    appUpdateResult: { value: null },
    appUpdateError: { value: "" },
    appUpdateVisible: { value: false },
    appUpdateDownloading: { value: false },
    appUpdateCancelling: { value: false },
    appUpdateOpening: { value: false },
  };
  const calls: string[] = [];
  const successMessages: string[] = [];
  const errorMessages: string[] = [];
  const backend = {
    async downloadAppUpdate() { calls.push("download"); return { path, assetName, version: "1.2.3", sizeBytes: 10 }; },
    async installAppUpdateAndRestart(path: string) { calls.push(`install:${path}`); },
    async openAppUpdateInstaller(path: string) { calls.push(`open:${path}`); },
    async openPathInFileManager(path: string) { calls.push(`folder:${path}`); },
    async cancelAppUpdateDownload() { calls.push("cancel"); },
  };
  const context = {
    ...refs, ...backend,
    t: (text: string) => text, errorText: String,
    Message: { success(message: string) { successMessages.push(message); }, error(message: string) { errorMessages.push(message); } },
  };
  const api = runInNewContext(`${compiled}\n({ startAppUpdateDownload, openDownloadedAppUpdate, openAppUpdateFolder, closeAppUpdateModal })`, context) as {
    startAppUpdateDownload(): Promise<void>;
    openDownloadedAppUpdate(): Promise<void>;
    openAppUpdateFolder(): Promise<void>;
    closeAppUpdateModal(): void;
  };
  return { ...refs, api, calls, context, path, successMessages, errorMessages };
}

test("macOS 下载成功后自动安装重启，重启前禁止重复安装和关闭弹窗", async () => {
  const state = fixture();
  await state.api.startAppUpdateDownload();
  assert.deepEqual(state.calls, ["download", "install:/updates/1.2.3/update.dmg"]);
  assert.equal(state.appUpdateOpening.value, true);
  assert.equal(state.appUpdateDownloading.value, false);
  await state.api.startAppUpdateDownload();
  await state.api.openDownloadedAppUpdate();
  state.api.closeAppUpdateModal();
  assert.equal(state.calls.length, 2);
  assert.equal(state.appUpdateVisible.value, true);
});

test("Windows 下载后仍由用户打开安装器", async () => {
  const state = fixture("windows-x86_64");
  await state.api.startAppUpdateDownload();
  assert.deepEqual(state.calls, ["download"]);
  assert.equal(state.appUpdateOpening.value, false);
  await state.api.openDownloadedAppUpdate();
  assert.deepEqual(state.calls, ["download", `open:${state.path}`]);
  assert.deepEqual(state.successMessages, ["已请求系统打开安装包，请按安装程序完成更新。"]);
  assert.equal(state.appUpdateOpening.value, false);
  state.api.closeAppUpdateModal();
  assert.equal(state.appUpdateVisible.value, false);
});

test("Windows 启动被拒绝时保留下载文件、显示错误且不提示成功", async () => {
  const state = fixture("windows-x86_64");
  await state.api.startAppUpdateDownload();
  const downloaded = state.appUpdateResult.value;
  state.context.openAppUpdateInstaller = async () => { throw "操作被用户取消 (os error 1223)"; };
  await state.api.openDownloadedAppUpdate();
  assert.match(state.appUpdateError.value, /打开安装包失败.*1223/);
  assert.equal(state.appUpdateResult.value, downloaded);
  assert.equal(state.appUpdateOpening.value, false);
  assert.deepEqual(state.successMessages, []);

  await state.api.openAppUpdateFolder();
  assert.deepEqual(state.calls, ["download", `folder:${state.path}`]);
  assert.equal(state.appUpdateResult.value, downloaded);
});

test("打开更新文件夹失败时反馈错误，且不会触发安装或重新下载", async () => {
  const state = fixture("windows-x86_64");
  await state.api.openAppUpdateFolder();
  assert.deepEqual(state.calls, []);
  await state.api.startAppUpdateDownload();
  state.context.openPathInFileManager = async () => { throw "找不到文件夹"; };
  await state.api.openAppUpdateFolder();
  assert.deepEqual(state.calls, ["download"]);
  assert.deepEqual(state.errorMessages, ["打开文件夹失败：找不到文件夹"]);
  assert.ok(state.appUpdateResult.value);
});

test("安装失败保留已下载文件并允许直接重试安装", async () => {
  const state = fixture();
  state.context.installAppUpdateAndRestart = async () => { throw new Error("不可写"); };
  await state.api.startAppUpdateDownload();
  assert.match(state.appUpdateError.value, /更新安装失败.*不可写/);
  assert.ok(state.appUpdateResult.value);
  assert.equal(state.appUpdateOpening.value, false);
  state.context.installAppUpdateAndRestart = async () => { state.calls.push("retry-install"); };
  await state.api.openDownloadedAppUpdate();
  assert.equal(state.appUpdateError.value, "");
  assert.deepEqual(state.calls, ["download", "retry-install"]);
});

test("下载失败或取消时不会安装，也不会卡在忙碌状态", async () => {
  for (const message of ["下载已取消", "连接失败"]) {
    const state = fixture();
    state.context.downloadAppUpdate = async () => { throw new Error(message); };
    // IPC rejection is a plain string, as in the actual Tauri command.
    state.context.errorText = (error: unknown) => error instanceof Error ? error.message : String(error);
    await state.api.startAppUpdateDownload();
    assert.equal(state.calls.length, 0);
    assert.equal(state.appUpdateDownloading.value, false);
    assert.equal(state.appUpdateOpening.value, false);
    assert.equal(state.appUpdateError.value, message === "下载已取消" ? "" : message);
  }
});
