<script setup lang="ts">
import { ref } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
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

async function add(): Promise<void> {
  if (pending.value) return;
  pending.value = "import";
  try {
    const result = await importExtensions();
    failures.value = result.failed;
    if (result.installed.length) {
      const byId = new Map(extensions.value.map((item) => [item.id, item]));
      for (const item of result.installed) byId.set(item.id, item);
      extensions.value = [...byId.values()].sort((a, b) => a.name.localeCompare(b.name));
      Message.success(t("插件已安装"));
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
      <a-button type="primary" :loading="pending === 'import'" :disabled="!!pending" @click="add">
        <template #icon><icon-plus /></template>
        {{ t("添加插件") }}
      </a-button>
    </header>
    <a-alert v-if="failures.length" type="error" class="plugin-import-errors" :title="t('无法导入插件')">
      <ul>
        <li v-for="({ file, error }, index) in failures" :key="index"><strong>{{ file }}</strong> · {{ t(error) }}</li>
      </ul>
    </a-alert>
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
.plugin-import-errors { margin-bottom: 12px; }
.plugin-import-errors ul { margin: 4px 0; padding-inline-start: 18px; overflow-wrap: anywhere; }
</style>
