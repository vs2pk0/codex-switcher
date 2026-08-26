<script setup lang="ts">
import { h, onMounted, reactive, ref } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import { open } from "@tauri-apps/plugin-dialog";
import {
  deleteCodexInstance,
  instanceDisplayName,
  launchCodexInstance,
  listCodexInstances,
  restartCodexInstance,
  saveCodexInstance,
  stopCodexInstance,
  type CodexInstance,
} from "../services/instances";
import { openPathInFileManager } from "../services/session";
import { formatTranslatedText, t } from "../i18n";

const emit = defineEmits<{ (event: "instances-updated", instances: CodexInstance[]): void }>();
const instances = ref<CodexInstance[]>([]);
const loading = ref(false);
const workingId = ref("");
const editorVisible = ref(false);
const saving = ref(false);
const form = reactive({
  id: "",
  name: "",
  codexHome: "",
  electronData: "",
  appPath: "/Applications/ChatGPT.app",
  workspace: "",
});

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function refresh(): Promise<void> {
  loading.value = true;
  try {
    instances.value = await listCodexInstances();
    emit("instances-updated", instances.value);
  } catch (error) {
    Message.error(formatTranslatedText("读取实例失败：{error}", { error: errorText(error) }));
  } finally {
    loading.value = false;
  }
}

function createInstance(): void {
  const defaultAppPath = instances.value.find((instance) => instance.isDefault)?.appPath
    || "/Applications/ChatGPT.app";
  Object.assign(form, {
    id: "",
    name: formatTranslatedText("多开实例 {count}", { count: Math.max(1, instances.value.length) }),
    codexHome: "",
    electronData: "",
    appPath: defaultAppPath,
    workspace: "",
  });
  editorVisible.value = true;
}

function editInstance(instance: CodexInstance): void {
  Object.assign(form, {
    id: instance.id,
    name: instance.name,
    codexHome: instance.codexHome,
    electronData: instance.electronData,
    appPath: instance.appPath,
    workspace: instance.workspace || "",
  });
  editorVisible.value = true;
}

async function chooseDirectory(field: "codexHome" | "electronData" | "workspace"): Promise<void> {
  const selected = await open({ directory: true, multiple: false, canCreateDirectories: true });
  if (typeof selected === "string") form[field] = selected;
}

async function chooseApp(): Promise<void> {
  const selected = await open({ directory: false, multiple: false, filters: [{ name: "macOS App", extensions: ["app"] }] });
  if (typeof selected === "string") form.appPath = selected;
}

async function save(): Promise<void> {
  saving.value = true;
  try {
    await saveCodexInstance({
      id: form.id || null,
      name: form.name,
      codexHome: form.codexHome || null,
      electronData: form.electronData || null,
      appPath: form.appPath,
      workspace: form.workspace || null,
    });
    editorVisible.value = false;
    await refresh();
    Message.success(t(form.id ? "实例配置已保存" : "多开实例已创建"));
  } catch (error) {
    Message.error(formatTranslatedText("保存实例失败：{error}", { error: errorText(error) }));
  } finally {
    saving.value = false;
  }
}

async function runAction(instance: CodexInstance, action: "launch" | "stop" | "restart"): Promise<void> {
  if (workingId.value) return;
  workingId.value = instance.id;
  try {
    if (action === "launch") await launchCodexInstance(instance.id);
    if (action === "stop") await stopCodexInstance(instance.id);
    if (action === "restart") await restartCodexInstance(instance.id);
    await refresh();
    Message.success(formatTranslatedText("{name} 已{action}", {
      name: t(instanceDisplayName(instance)),
      action: t(action === "launch" ? "启动" : action === "stop" ? "停止" : "重启"),
    }));
  } catch (error) {
    Message.error(formatTranslatedText("实例操作失败：{error}", { error: errorText(error) }));
  } finally {
    workingId.value = "";
  }
}

function confirmDelete(instance: CodexInstance): void {
  Modal.warning({
    title: t("永久删除多开实例"),
    content: () => h("div", { style: { display: "grid", gap: "10px" } }, [
      h("p", { style: { margin: 0 } }, formatTranslatedText("确认永久删除“{name}”及其全部实例数据？", { name: instance.name })),
      h("div", { style: { display: "grid", gap: "6px" } }, [
        h("div", [h("strong", "Codex Home："), h("code", { style: { wordBreak: "break-all" } }, instance.codexHome)]),
        h("div", [h("strong", t("桌面数据：")), h("code", { style: { wordBreak: "break-all" } }, instance.electronData)]),
      ]),
      h("p", { style: { margin: 0 } }, `${t("同时清理该实例的托管目录、会话回收站、配置备份和可确认归属的手动会话备份。")}${instance.running ? t("实例当前正在运行，将先自动停止。") : ""}`),
      h("p", { style: { margin: 0, color: "#ef4444", fontWeight: 600 } }, formatTranslatedText("工作区{workspace}、官方 App、系统默认实例和其他实例不会删除。此操作不可恢复。", {
        workspace: instance.workspace ? `（${instance.workspace}）` : "",
      })),
    ]),
    okText: t("永久删除"),
    cancelText: t("取消"),
    hideCancel: false,
    async onBeforeOk() {
      if (workingId.value) return false;
      workingId.value = instance.id;
      try {
        const result = await deleteCodexInstance(instance.id);
        await refresh();
        Message.success(formatTranslatedText("实例已彻底删除：清理 {directoryCount} 个数据目录、{backupCount} 个关联备份", {
          directoryCount: result.deletedPaths.length,
          backupCount: result.deletedBackupCount,
        }));
      } catch (error) {
        Message.error(formatTranslatedText("删除实例失败：{error}", { error: errorText(error) }));
        return false;
      } finally {
        workingId.value = "";
      }
      return true;
    },
  });
}

onMounted(refresh);
</script>

<template>
  <section class="instances-page">
    <header class="instances-hero">
      <div>
        <h1>{{ t("Codex 多开") }}</h1>
        <p>{{ t("为每个桌面实例隔离账号、配置、会话、插件与 Electron 数据。") }}</p>
      </div>
      <div class="instances-hero-actions">
        <a-button :loading="loading" @click="refresh"><template #icon><icon-refresh /></template>{{ t("刷新") }}</a-button>
        <a-button type="primary" @click="createInstance"><template #icon><icon-plus /></template>{{ t("新建多开实例") }}</a-button>
      </div>
    </header>

    <a-alert type="info" show-icon>
      {{ t("“系统默认实例”代表当前原版 Codex。新增实例会使用独立 CODEX_HOME 和桌面数据目录；删除多开实例会永久清理该实例的数据和关联备份，但不会删除工作区、官方 App、系统默认实例或其他实例。") }}
    </a-alert>

    <a-spin :loading="loading" dot>
      <div class="instance-grid">
        <article v-for="instance in instances" :key="instance.id" class="instance-card" :class="{ running: instance.running }">
          <header>
            <span class="instance-status" />
            <div class="instance-card-heading">
              <h3>{{ t(instanceDisplayName(instance)) }}</h3>
              <p>{{ instance.running ? `${t("运行中")} · PID ${instance.pid}` : t("未运行") }}</p>
            </div>
            <div class="instance-card-tags">
              <a-tag v-if="instance.openCodexConnected" color="purple">
                <template #icon><icon-link /></template>
                OpenCodex
              </a-tag>
              <a-tag v-if="instance.isDefault" color="blue">{{ t("默认") }}</a-tag>
            </div>
          </header>
          <dl>
            <div><dt>Codex Home</dt><dd :title="instance.codexHome">{{ instance.codexHome }}</dd><button @click="openPathInFileManager(instance.codexHome)"><icon-folder /></button></div>
            <div><dt>{{ t("桌面数据") }}</dt><dd :title="instance.electronData">{{ instance.electronData }}</dd><button @click="openPathInFileManager(instance.electronData)"><icon-folder /></button></div>
            <div><dt>{{ t("工作区") }}</dt><dd :title="instance.workspace || t('未设置')">{{ instance.workspace || t("未设置") }}</dd></div>
          </dl>
          <footer>
            <a-button v-if="!instance.running" type="primary" :loading="workingId === instance.id" @click="runAction(instance, 'launch')"><template #icon><icon-play-arrow /></template>{{ t("启动") }}</a-button>
            <a-button v-else status="warning" :loading="workingId === instance.id" @click="runAction(instance, 'restart')"><template #icon><icon-refresh /></template>{{ t("重启") }}</a-button>
            <a-button v-if="instance.running" :disabled="Boolean(workingId)" @click="runAction(instance, 'stop')"><template #icon><icon-pause /></template>{{ t("停止") }}</a-button>
            <a-button v-if="!instance.isDefault" :disabled="instance.running" @click="editInstance(instance)"><template #icon><icon-edit /></template>{{ t("编辑") }}</a-button>
            <a-button v-if="!instance.isDefault" status="danger" :loading="workingId === instance.id" :disabled="Boolean(workingId)" @click="confirmDelete(instance)"><template #icon><icon-delete /></template></a-button>
          </footer>
        </article>
      </div>
    </a-spin>

    <a-modal v-model:visible="editorVisible" :title="t(form.id ? '编辑多开实例' : '新建多开实例')" :ok-text="t('保存')" :cancel-text="t('取消')" :ok-loading="saving" width="720px" @ok="save">
      <a-form :model="form" layout="vertical">
        <a-form-item :label="t('实例名称')" required><a-input v-model="form.name" :placeholder="t('例如：工作账号')" /></a-form-item>
        <a-form-item label="Codex Home"><a-input v-model="form.codexHome" :placeholder="t('留空自动生成')"><template #append><a-button @click="chooseDirectory('codexHome')"><icon-folder /></a-button></template></a-input></a-form-item>
        <a-form-item :label="t('桌面数据目录')"><a-input v-model="form.electronData" :placeholder="t('留空自动生成')"><template #append><a-button @click="chooseDirectory('electronData')"><icon-folder /></a-button></template></a-input></a-form-item>
        <a-form-item :label="t('工作区（可选）')"><a-input v-model="form.workspace" :placeholder="t('启动时打开的项目目录')"><template #append><a-button @click="chooseDirectory('workspace')"><icon-folder /></a-button></template></a-input></a-form-item>
        <a-form-item :label="t('官方 App')"><a-input v-model="form.appPath"><template #append><a-button @click="chooseApp"><icon-folder /></a-button></template></a-input></a-form-item>
      </a-form>
    </a-modal>
  </section>
</template>

<style scoped>
.instances-page { display: grid; gap: 18px; }
.instances-hero { display: flex; align-items: center; justify-content: space-between; gap: 24px; }
.instances-hero h1 { margin: 0; color: #111827; font-size: clamp(28px, 3vw, 42px); letter-spacing: -.04em; }
.instances-hero p { margin: 7px 0 0; color: #64748b; font-size: 15px; }
.instances-hero-actions { display: flex; gap: 10px; }
.instance-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(420px, 1fr)); align-items: stretch; gap: 16px; }
.instance-card { display: grid; grid-template-rows: auto 1fr auto; overflow: hidden; min-width: 0; border: 1px solid #dbe5f2; border-radius: 16px; background: rgba(255,255,255,.88); box-shadow: 0 12px 32px rgba(37, 59, 93, .06); }
.instance-card.running { border-color: rgba(34,197,94,.46); }
.instance-card > header { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; min-height: 76px; gap: 12px; box-sizing: border-box; padding: 16px 20px; border-bottom: 1px solid #e8eef6; }
.instance-card-heading { min-width: 0; }.instance-card h3 { overflow: hidden; margin: 0; font-size: 17px; text-overflow: ellipsis; white-space: nowrap; }.instance-card header p { margin: 4px 0 0; color: #75849a; font-size: 12px; }
.instance-card-tags { display: flex; flex: none; align-items: center; justify-content: flex-end; gap: 6px; white-space: nowrap; }
.instance-status { width: 11px; height: 11px; border-radius: 50%; background: #aab5c4; }.running .instance-status { background: #22c55e; box-shadow: 0 0 0 5px rgba(34,197,94,.12); }
.instance-card dl { display: grid; gap: 10px; margin: 0; padding: 18px 20px; }
.instance-card dl div { display: grid; grid-template-columns: 92px minmax(0, 1fr) 28px; align-items: center; gap: 10px; }.instance-card dt { color: #64748b; font-size: 12px; font-weight: 700; }.instance-card dd { overflow: hidden; margin: 0; color: #334155; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }.instance-card dl button { border: 0; color: #64748b; background: transparent; cursor: pointer; }
.instance-card footer { display: flex; align-items: center; min-height: 60px; gap: 8px; box-sizing: border-box; padding: 12px 20px; border-top: 1px solid #e8eef6; background: #f8fafc; }
@media (max-width: 560px) {
  .instance-grid { grid-template-columns: minmax(0, 1fr); }
  .instance-card > header { grid-template-columns: auto minmax(0, 1fr); }
  .instance-card-tags { grid-column: 2; justify-content: flex-start; }
  .instance-card footer { flex-wrap: wrap; }
}
</style>
