<script setup lang="ts">
import { ref, watch } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import { isTauri } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { t } from "../i18n";
import {
  importExtensions,
  setExtensionEnabled,
  uninstallExtension,
  type ExtensionImportResult,
  type InstalledExtension,
} from "../services/extensions";

const extensions = defineModel<InstalledExtension[]>({ required: true });
const pending = ref("");
const failures = ref<ExtensionImportResult["failed"]>([]);
const addVisible = ref(false);
const dropArea = ref<HTMLElement | null>(null);
const dragOver = ref(false);
let draggingFiles = false;

function openAddDialog(): void {
  failures.value = [];
  addVisible.value = true;
}

function closeAddDialog(): void {
  if (!pending.value) addVisible.value = false;
}

function handleFileDrag(payload: DragDropEvent): void {
  const area = dropArea.value;
  if (payload.type === "enter") draggingFiles = payload.paths.length > 0;
  if (payload.type === "leave" || payload.type === "drop") draggingFiles = false;
  if (!addVisible.value || !area || pending.value || payload.type === "leave") {
    dragOver.value = false;
    return;
  }
  // Wry reports coordinates in device pixels on Windows and in view units on macOS and Linux.
  const scale = navigator.platform.startsWith("Win") ? window.devicePixelRatio : 1;
  const { x, y } = payload.position.toLogical(scale);
  const { left, right, top, bottom } = area.getBoundingClientRect();
  const inside = x >= left && x <= right && y >= top && y <= bottom;
  dragOver.value = inside && draggingFiles;
  if (inside && payload.type === "drop" && payload.paths.length) void add(payload.paths);
}

watch(addVisible, async (visible, _, onCleanup) => {
  dragOver.value = false;
  draggingFiles = false;
  if (!visible || !isTauri()) return;
  let closed = false;
  let unsubscribe: UnlistenFn | undefined;
  onCleanup(() => {
    closed = true;
    unsubscribe?.();
  });
  try {
    unsubscribe = await getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (!closed) handleFileDrag(payload);
    });
    if (closed) unsubscribe();
  } catch {
    if (!closed) Message.warning(t("拖放不可用，请选择插件文件。"));
  }
});

async function add(paths?: string[]): Promise<void> {
  if (pending.value) return;
  dragOver.value = false;
  pending.value = "import";
  try {
    const result = await importExtensions(paths);
    failures.value = result.failed;
    if (result.installed.length) {
      const byId = new Map(extensions.value.map((item) => [item.id, item]));
      for (const item of result.installed) byId.set(item.id, item);
      extensions.value = [...byId.values()].sort((a, b) => a.name.localeCompare(b.name));
      Message.success(t("插件已安装"));
      if (!result.failed.length) addVisible.value = false;
    }
  } catch (error) {
    Message.error(t(String(error)));
  } finally {
    pending.value = "";
  }
}

async function toggle(extension: InstalledExtension, enabled: boolean): Promise<void> {
  if (pending.value) return;
  pending.value = extension.id;
  try {
    await setExtensionEnabled(extension.id, enabled);
    extensions.value = extensions.value.map((item) => item.id === extension.id ? { ...item, enabled } : item);
  } catch (error) {
    Message.error(t(String(error)));
  } finally {
    pending.value = "";
  }
}

function remove(extension: InstalledExtension): void {
  Modal.confirm({
    title: `${t("卸载插件")} · ${t(extension.name)}`,
    content: t("账号和用量记录会保留在本机。"),
    okText: t("卸载插件"),
    okButtonProps: { status: "danger" },
    async onBeforeOk() {
      if (pending.value) return false;
      pending.value = extension.id;
      try {
        await uninstallExtension(extension.id);
        extensions.value = extensions.value.filter(({ id }) => id !== extension.id);
        return true;
      } catch (error) {
        Message.error(t(String(error)));
        return false;
      } finally {
        pending.value = "";
      }
    },
  });
}
</script>

<template>
  <section class="usage-panel plugin-manager">
    <header class="usage-head">
      <h2>{{ t("插件管理") }}</h2>
      <a-button type="primary" :disabled="!!pending" @click="openAddDialog">
        <template #icon><icon-plus /></template>
        {{ t("添加插件") }}
      </a-button>
    </header>
    <div v-for="extension in extensions" :key="extension.id" class="plugin-manager-row">
      <span class="plugin-manager-icon" aria-hidden="true"><icon-apps v-if="extension.usage" /><icon-common v-else /></span>
      <div class="plugin-manager-name">
        <strong>{{ t(extension.name) }}</strong>
        <span>v{{ extension.version }}</span>
      </div>
      <div class="plugin-manager-actions">
        <a-switch
          :model-value="extension.enabled !== false"
          :disabled="!!pending"
          :aria-label="`${t('启用插件')} · ${t(extension.name)}`"
          @change="toggle(extension, !!$event)"
        />
        <a-button type="text" status="danger" :disabled="!!pending" @click="remove(extension)">
          <template #icon><icon-delete /></template>
          {{ t("卸载插件") }}
        </a-button>
      </div>
    </div>
    <a-empty v-if="!extensions.length" :description="t('暂无已安装插件')" />

    <a-modal
      :visible="addVisible"
      :title="t('添加插件')"
      :mask-closable="false"
      :closable="!pending"
      :esc-to-close="!pending"
      :footer="false"
      unmount-on-close
      @cancel="closeAddDialog"
    >
      <div
        ref="dropArea"
        class="plugin-drop-area"
        :class="{ 'is-drag-over': dragOver }"
        :aria-busy="pending === 'import'"
      >
        <span class="plugin-drop-icon" aria-hidden="true"><icon-upload /></span>
        <div>
          <strong>{{ t("将插件包拖到此处") }}</strong>
          <p>{{ t("支持一次添加多个 .csplugin 文件") }}</p>
        </div>
        <a-button type="outline" :loading="pending === 'import'" @click="add()">
          <template #icon><icon-folder /></template>
          {{ t("选择插件文件") }}
        </a-button>
      </div>
      <a-alert v-if="failures.length" type="error" class="plugin-import-errors" :title="t('无法导入插件')">
        <ul>
          <li v-for="({ file, error }, index) in failures" :key="index"><strong>{{ file }}</strong> · {{ t(error) }}</li>
        </ul>
      </a-alert>
    </a-modal>
  </section>
</template>

<style scoped>
.plugin-manager-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 14px 0;
}
.plugin-manager-row + .plugin-manager-row { border-top: 1px solid var(--line); }
.plugin-manager-icon {
  display: grid;
  place-items: center;
  flex: 0 0 40px;
  height: 40px;
  border-radius: 8px;
  background: #eef4ff;
  color: rgb(var(--primary-6));
  font-size: 19px;
}
.plugin-manager-name {
  display: flex;
  flex: 1;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
  overflow-wrap: anywhere;
}
.plugin-manager-name span { color: var(--text-soft); font-size: 12px; }
.plugin-manager-actions { display: flex; align-items: center; gap: 12px; }
.plugin-drop-area {
  display: grid;
  min-height: 216px;
  justify-items: center;
  align-content: center;
  gap: 14px;
  padding: 24px;
  border: 1px dashed var(--line-strong);
  border-radius: 12px;
  background: var(--color-fill-1);
  text-align: center;
}
.plugin-drop-area.is-drag-over {
  border-color: rgb(var(--primary-6));
  background: rgb(var(--primary-1));
}
.plugin-drop-icon {
  display: grid;
  width: 48px;
  height: 48px;
  place-items: center;
  border-radius: 12px;
  background: rgb(var(--primary-1));
  color: rgb(var(--primary-6));
  font-size: 24px;
}
.plugin-drop-area strong { color: var(--text-main); font-size: 16px; }
.plugin-drop-area p { margin: 4px 0 0; color: var(--text-soft); font-size: 13px; }
.plugin-import-errors { margin-top: 12px; }
.plugin-import-errors ul {
  max-height: 180px;
  margin: 4px 0;
  padding-inline-start: 18px;
  overflow: auto;
  overflow-wrap: anywhere;
}
</style>
