<script setup lang="ts">
import { computed, defineAsyncComponent, reactive, watch } from "vue";
import { t } from "../i18n";
import {
  listExtensionUsageApps,
  syncExtensionUsage,
  usageAppNames,
  type InstalledExtension,
  type UsageSyncResult,
} from "../services/extensions";
import UsageAppIcon from "./UsageAppIcon.vue";

const ExtensionUsagePanel = defineAsyncComponent(() => import("./ExtensionUsagePanel.vue"));
const props = defineProps<{
  extensions: InstalledExtension[];
  accountExtensions: InstalledExtension[];
  active: boolean;
}>();
const selection = defineModel<string>({ required: true });

type UsageState = {
  apps: string[];
  result: UsageSyncResult | null;
  syncing: boolean;
  error: string;
};

const states = reactive(new Map<string, UsageState>());
const opened = reactive(new Set<string>());
const accountPlatformByApp: Record<string, string> = {
  claude: "claude",
  grokbuild: "grok",
  mcode: "minimax_code",
  agy: "antigravity",
  copilot: "github_copilot",
  workbuddy: "workbuddy",
};
const accountIcons = computed(() => {
  const icons = new Map(props.accountExtensions.flatMap(({ platforms }) =>
    platforms.map(({ id, iconPng }) => [id, iconPng] as const),
  ));
  return Object.fromEntries(Object.entries(accountPlatformByApp).flatMap(([app, platform]) => {
    const icon = icons.get(platform);
    return icon ? [[app, icon]] : [];
  }));
});
const revision = (extension: InstalledExtension) => `${extension.id}:${extension.version}`;
const packages = computed(() => props.extensions.map((extension) => {
  const key = revision(extension);
  const state = states.get(key);
  const isPlatformUsage = extension.id === "platform-usage";
  const icons = { ...(isPlatformUsage ? accountIcons.value : {}), ...extension.usageIcons };
  const apps = [...new Set([
    ...(isPlatformUsage ? Object.keys(usageAppNames) : []),
    ...Object.keys(extension.usageIcons ?? {}),
    ...(state?.apps ?? []),
  ])];
  return { extension, key, state, icons, apps, isPlatformUsage };
}));
const tabs = computed(() => packages.value.flatMap(({ extension, icons, apps, isPlatformUsage }) => {
  const name = t(extension.name);
  return [
    { key: extension.id, extensionId: extension.id, app: "", label: isPlatformUsage ? t("全部") : name, title: name, iconPng: undefined },
    ...apps.map((app) => ({
      key: `${extension.id}:${app}`,
      extensionId: extension.id,
      app,
      label: usageAppNames[app] ?? app,
      title: app === "codex" ? `Codex · ${t("会话日志")}` : usageAppNames[app] ?? app,
      iconPng: icons[app],
    })),
  ];
}));
const activeTab = computed(() => tabs.value.find(({ key }) => key === selection.value));

async function sync(extension: InstalledExtension, state: UsageState, discover = false): Promise<void> {
  if (state.syncing) return;
  state.syncing = true;
  state.error = "";
  try {
    if (discover && extension.usageAppDiscovery) {
      state.apps = await listExtensionUsageApps(extension.id).catch(() => []);
      if (!props.extensions.some((item) => revision(item) === revision(extension))) return;
    }
    const result = await syncExtensionUsage(extension.id);
    if (result.applications) state.apps = result.applications;
    state.result = result;
  } catch (cause) {
    state.error = String(cause);
  } finally {
    state.syncing = false;
  }
}

watch([() => props.active, () => props.extensions.map(revision).join("|")], ([active]) => {
  const available = new Set(props.extensions.map(revision));
  for (const key of states.keys()) if (!available.has(key)) states.delete(key);
  for (const id of opened) {
    if (!props.extensions.some((extension) => extension.id === id)) opened.delete(id);
  }
  if (!active) return;
  for (const { extension, key } of packages.value) {
    if (states.has(key)) continue;
    const state = reactive<UsageState>({ apps: [], result: null, syncing: false, error: "" });
    states.set(key, state);
    void sync(extension, state, true);
  }
}, { immediate: true });

watch(activeTab, (tab) => {
  if (tab) opened.add(tab.extensionId);
  else if (selection.value) selection.value = "";
}, { immediate: true });
</script>

<template>
  <a-tabs
    v-if="tabs.length"
    :active-key="selection"
    class="usage-extension-switch"
    type="rounded"
    size="small"
    :header-padding="false"
    hide-content
    @change="selection = String($event)"
  >
    <a-tab-pane key="">
      <template #title><span class="usage-app-cell"><UsageAppIcon app="codex" /> Codex</span></template>
    </a-tab-pane>
    <a-tab-pane v-for="tab in tabs" :key="tab.key">
      <template #title>
        <span class="usage-app-cell usage-platform-tab" :title="tab.title">
          <UsageAppIcon :app="tab.app" :icon-png="tab.iconPng" />
          <span :class="{ 'usage-tab-label': tab.app }">{{ tab.app ? tab.title : tab.label }}</span>
        </span>
      </template>
    </a-tab-pane>
  </a-tabs>

  <slot name="codex" :visible="!activeTab" :active="active && !activeTab" />
  <template v-for="{ extension, key, state, icons } in packages" :key="key">
    <ExtensionUsagePanel
      v-if="state && opened.has(extension.id)"
      v-show="activeTab?.extensionId === extension.id"
      :extension="extension"
      :app-icons="icons"
      :app="activeTab?.extensionId === extension.id ? activeTab.app : ''"
      :active="active && activeTab?.extensionId === extension.id"
      :syncing="state.syncing"
      :sync-result="state.result"
      :sync-error="state.error"
      @sync="sync(extension, state)"
    />
  </template>
</template>

<style scoped>
.usage-platform-tab {
  position: relative;
  min-height: 21px;
  font-size: 15px;
}
.usage-platform-tab:not(:has(.usage-tab-label)) {
  font-size: 13px;
}
.usage-tab-label {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
</style>
