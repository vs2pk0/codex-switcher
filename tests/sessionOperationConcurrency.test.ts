import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import test from "node:test";
import ts from "typescript";
import { computed, ref } from "vue";

const source = readFileSync(new URL("../src/App.vue", import.meta.url), "utf8");
function section(start: string, end: string): string {
  const offset = source.indexOf(start);
  const limit = source.indexOf(end, offset);
  assert.ok(offset >= 0 && limit > offset);
  return source.slice(offset, limit);
}
const compiled = ts.transpileModule([
  section("const accountOrSessionOperationBusy =", "const sessionHistoryRepairVisible ="),
  section("async function handleSwitch(", "/** API Key 账号同步配置"),
  section("function confirmRepairSessionModels(", "/** 一键修复："),
  section("async function runOneClickRepairSessions(", "/** 一键修复完成后的二次确认"),
].join("\n"), { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

function fixture() {
  const calls: string[] = [];
  const dialogs: Array<{ onOk: () => Promise<void> }> = [];
  const account = { id: "account", email: "fixture@example.com" };
  const context = {
    computed,
    switchingId: ref(""),
    sessionRepairing: ref(false),
    repairingSessionId: ref(""),
    sessionModelRepairing: ref(false),
    sessionOneClickRepairing: ref(false),
    backupWorking: ref(false),
    appBusy: ref<unknown>(null),
    codexInstances: ref([{ id: "default", name: "Codex" }]),
    pendingSwitchAccount: ref<unknown>(account),
    accountInstancePickerVisible: ref(false),
    currentAccount: ref<unknown>(null),
    sessionInstanceId: ref("default"),
    t: (value: string) => value,
    errorText: String,
    displayName: () => "Fixture",
    isApiKeyAccount: () => false,
    oneClickRepairSteps: () => ["Close", "Repair", "Restore"],
    Message: { error() {}, warning() {}, success() {} },
    Modal: { warning(options: { onOk: () => Promise<void> }) { dialogs.push(options); } },
    async loadAccounts() {},
    async loadSessions() {},
    async loadCodexInstances() {},
    async askRestartCodexAfterRepair() { return false; },
    async switchCodexAccount() { calls.push("switch"); return { account }; },
    async oneClickRepairCodexSessions() {
      calls.push("repair");
      return { warnings: [], message: "done" };
    },
    async repairCodexSessionModelCompatibility() {
      calls.push("model-repair");
      return { warnings: [], repairedRolloutFileCount: 0, repairedThreadCount: 0, synchronizedCatalogRowCount: 0 };
    },
  };
  const handlers = runInNewContext(`${compiled}\n({ handleSwitch, performAccountSwitch, runOneClickRepairSessions, confirmRepairSessionModels })`, context) as {
    handleSwitch(account: unknown): Promise<void>;
    performAccountSwitch(instanceId: string): Promise<void>;
    runOneClickRepairSessions(instanceId: string, instanceName: string): Promise<void>;
    confirmRepairSessionModels(): void;
  };
  return { context, handlers, calls, dialogs, account };
}

test("大修复进行中拒绝重复修复与账号切换，失败后允许重试", async () => {
  const state = fixture();
  const pending = deferred<{ warnings: never[]; message: string }>();
  state.context.oneClickRepairCodexSessions = async () => {
    state.calls.push("repair");
    return pending.promise;
  };
  const first = state.handlers.runOneClickRepairSessions("default", "Codex");
  await state.handlers.runOneClickRepairSessions("default", "Codex");
  await state.handlers.performAccountSwitch("default");
  assert.deepEqual(state.calls, ["repair"]);
  pending.reject(new Error("fixture failure"));
  await first;
  assert.equal(state.context.appBusy.value, null);
  assert.equal(state.context.sessionOneClickRepairing.value, false);
  await state.handlers.performAccountSwitch("default");
  assert.deepEqual(state.calls, ["repair", "switch"]);
});

test("已打开的修复确认框会在执行时重新检查账号切换状态", async () => {
  const state = fixture();
  state.handlers.confirmRepairSessionModels();
  assert.equal(state.dialogs.length, 1);
  const pending = deferred<{ account: typeof state.account }>();
  state.context.switchCodexAccount = async () => {
    state.calls.push("switch");
    return pending.promise;
  };
  const switching = state.handlers.performAccountSwitch("default");
  await state.dialogs[0].onOk();
  await state.handlers.runOneClickRepairSessions("default", "Codex");
  assert.deepEqual(state.calls, ["switch"]);
  pending.resolve({ account: state.account });
  await switching;
  await state.dialogs[0].onOk();
  assert.deepEqual(state.calls, ["switch", "model-repair"]);
  assert.equal(state.context.appBusy.value, null);
});

test("加载实例期间启动的会话修复也会阻止随后开始切换", async () => {
  const state = fixture();
  state.context.codexInstances.value = [];
  const pending = deferred<void>();
  state.context.loadCodexInstances = () => pending.promise;
  const switching = state.handlers.handleSwitch(state.account);
  state.context.sessionRepairing.value = true;
  pending.resolve();
  await switching;
  assert.deepEqual(state.calls, []);
  assert.equal(state.context.switchingId.value, "");
});
