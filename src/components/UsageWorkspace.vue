<script setup lang="ts">
import { computed, defineAsyncComponent, reactive, ref, watch } from "vue";
import { Message } from "@arco-design/web-vue";
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
const tabOrderStorageKey = "codex-switcher:usage-tab-order";
const tabOrders = ref(readTabOrders());
const sortExtensionId = ref("");
const draftOrder = ref<string[] | null>(null);
const draggedApp = ref("");
const hoveredApp = ref("");
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
    ...orderedApps(apps, tabOrders.value.get(extension.id)).map((app) => ({
      key: `${extension.id}:${app}`,
      extensionId: extension.id,
      app,
      label: appLabel(app),
      title: app === "codex" ? `Codex · ${t("会话日志")}` : appLabel(app),
      iconPng: icons[app],
    })),
  ];
}));
const activeTab = computed(() =>
  tabs.value.find(({ key }) => key === selection.value) ?? tabs.value[0],
);
const activePackage = computed(() => packages.value.find(({ extension }) => extension.id === activeTab.value?.extensionId));
const sortPackage = computed(() => packages.value.find(({ extension }) => extension.id === sortExtensionId.value));
const sortApps = computed(() => orderedApps(sortPackage.value?.apps ?? [], draftOrder.value));

function readTabOrders(): Map<string, string[]> {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(tabOrderStorageKey) ?? "{}");
    if (!stored || typeof stored !== "object" || Array.isArray(stored)) return new Map();
    return new Map(Object.entries(stored).flatMap(([id, apps]) =>
      Array.isArray(apps) ? [[id, [...new Set(apps.filter((app): app is string => typeof app === "string"))]]] : [],
    ));
  } catch {
    return new Map();
  }
}

function orderedApps(apps: string[], order?: string[] | null): string[] {
  if (!order?.length) return apps;
  const remaining = new Set(apps);
  return [...order.filter((app) => remaining.delete(app)), ...remaining];
}

function appLabel(app: string): string {
  return usageAppNames[app] ?? app;
}

function openSort(): void {
  if (!activePackage.value) return;
  const { id } = activePackage.value.extension;
  draftOrder.value = tabOrders.value.get(id) ?? null;
  sortExtensionId.value = id;
}

function resetDrag(): void {
  draggedApp.value = "";
  hoveredApp.value = "";
}

function closeSort(): void {
  sortExtensionId.value = "";
  resetDrag();
}

function moveApp(source: string, target: string): void {
  const apps = [...sortApps.value];
  const from = apps.indexOf(source);
  const to = apps.indexOf(target);
  if (from < 0 || to < 0 || from === to) return;
  apps.splice(from, 1);
  apps.splice(to, 0, source);
  draftOrder.value = apps;
}

function startDrag(event: PointerEvent, app: string): void {
  if (!event.isPrimary || event.button !== 0) return;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  draggedApp.value = app;
}

function appUnderPointer(event: PointerEvent): string {
  return document.elementFromPoint(event.clientX, event.clientY)
    ?.closest<HTMLElement>(".usage-sort-row")?.dataset.app ?? "";
}

function dropApp(event: PointerEvent): void {
  moveApp(draggedApp.value, appUnderPointer(event));
  resetDrag();
}

function saveSort(): void {
  const id = sortExtensionId.value;
  if (!sortPackage.value) return;
  if (draftOrder.value === null) {
    tabOrders.value.delete(id);
  } else {
    // Keep the cached positions of apps that discovery has not reported yet.
    const visible = new Set(sortApps.value);
    const reordered = sortApps.value.values();
    const cached = (tabOrders.value.get(id) ?? []).map((app) =>
      visible.has(app) ? reordered.next().value ?? app : app,
    );
    tabOrders.value.set(id, [...cached, ...reordered]);
  }
  try {
    localStorage.setItem(tabOrderStorageKey, JSON.stringify(Object.fromEntries(tabOrders.value)));
  } catch {
    Message.warning(t("排序已应用，本机缓存写入失败。"));
  }
  closeSort();
}

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
  selection.value = tab?.key ?? "";
  if (tab) opened.add(tab.extensionId);
}, { immediate: true });

watch([() => props.active, sortPackage], ([active, pkg]) => {
  if (!active || !pkg) closeSort();
});
</script>

<template>
  <a-tabs
    v-if="tabs.length"
    :active-key="activeTab?.key"
    class="usage-extension-switch"
    type="rounded"
    size="small"
    :header-padding="false"
    hide-content
    @change="selection = String($event)"
  >
    <template v-if="(activePackage?.apps.length ?? 0) > 1" #extra>
      <a-button type="text" size="small" class="usage-sort-button" aria-haspopup="dialog" @click="openSort">
        <template #icon><icon-sort /></template>
        {{ t("编辑排序") }}
      </a-button>
    </template>
    <a-tab-pane v-for="tab in tabs" :key="tab.key">
      <template #title>
        <span class="usage-app-cell usage-platform-tab" :title="tab.title">
          <UsageAppIcon :app="tab.app" :icon-png="tab.iconPng" />
          <span>{{ tab.label }}</span>
        </span>
      </template>
    </a-tab-pane>
  </a-tabs>

  <slot v-if="!tabs.length" name="codex" :active="active" />
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

  <a-modal
    :visible="!!sortPackage"
    :title="t('用量标签排序')"
    :ok-text="t('保存排序')"
    :cancel-text="t('取消')"
    unmount-on-close
    @ok="saveSort"
    @cancel="closeSort"
  >
    <p class="usage-sort-hint">{{ t("拖动或使用箭头调整标签顺序。") }}</p>
    <div class="sort-editor-list">
      <div
        v-for="(app, index) in sortApps"
        :key="app"
        class="sort-editor-row usage-sort-row"
        :class="{ dragging: draggedApp === app, over: hoveredApp === app && draggedApp !== app }"
        :data-app="app"
      >
        <span
          class="sort-editor-grip"
          :title="t('按住拖动排序')"
          aria-hidden="true"
          @pointerdown.prevent="startDrag($event, app)"
          @pointermove="draggedApp && (hoveredApp = appUnderPointer($event))"
          @pointerup="dropApp"
          @lostpointercapture="resetDrag"
        ><icon-drag-dot-vertical /></span>
        <span class="usage-app-cell"><UsageAppIcon :app="app" :icon-png="sortPackage?.icons[app]" />{{ appLabel(app) }}</span>
        <div class="sort-editor-actions">
          <a-button
            size="small"
            :disabled="index === 0"
            :aria-label="`${t('上移')} ${appLabel(app)}`"
            @click="moveApp(app, sortApps[index - 1])"
          ><template #icon><icon-up /></template></a-button>
          <a-button
            size="small"
            :disabled="index === sortApps.length - 1"
            :aria-label="`${t('下移')} ${appLabel(app)}`"
            @click="moveApp(app, sortApps[index + 1])"
          ><template #icon><icon-down /></template></a-button>
        </div>
      </div>
    </div>
    <a-button type="text" class="usage-sort-reset" @click="draftOrder = null">
      <template #icon><icon-undo /></template>
      {{ t("恢复默认") }}
    </a-button>
  </a-modal>
</template>

<style scoped>
.usage-platform-tab {
  min-height: 21px;
  font-size: 13px;
}
.usage-platform-tab :deep(.usage-app-icon) {
  width: 18px;
  height: 18px;
}
.usage-sort-button {
  margin-inline-start: 8px;
  white-space: nowrap;
}
.usage-sort-hint {
  margin: 0 0 10px;
  color: var(--text-soft);
}
.sort-editor-row.usage-sort-row {
  grid-template-columns: auto minmax(0, 1fr) auto;
  padding: 6px 8px;
  box-shadow: none;
}
.usage-sort-row.over {
  background: rgb(var(--primary-1));
  transform: none;
}
.usage-sort-row .usage-app-cell {
  overflow-wrap: anywhere;
}
.usage-sort-reset {
  margin-top: 10px;
}
</style>
