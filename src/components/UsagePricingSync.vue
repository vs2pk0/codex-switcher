<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Message } from "@arco-design/web-vue";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";
import { formatAppDateTime } from "../timeZone";
import { formatUnitPrice } from "../usageDisplay";
import { getCommonModelKeys, toModelPricing, type ModelsDevEntry, type ModelsDevSyncConfig } from "../modelsDevPricing";
import { fetchModelsDevEntries, importModelsDevPrices, loadModelsDevConfig, modelsDevSyncing, modelsDevSyncState, saveModelsDevConfig, syncModelsDevPricing } from "../services/modelsDev";

const visible = ref(false);
const mode = ref<"import" | "sync">("import");
const loading = ref(false);
const saving = ref(false);
const error = ref("");
const entries = ref<ModelsDevEntry[]>([]);
const search = ref("");
const provider = ref("all");
const selected = ref<string[]>([]);
const excluded = ref<string[]>([]);
const includeCommon = ref(true);
const config = computed(() => modelsDevSyncState.value?.config);
const common = computed(() => getCommonModelKeys(entries.value));
const selectedKeys = computed(() => {
  const keys = new Set(selected.value);
  if (mode.value === "sync" && includeCommon.value) for (const key of common.value) if (!excluded.value.includes(key)) keys.add(key);
  return keys;
});
const providers = computed(() => [...new Map(entries.value.map((entry) => [entry.providerId, entry.providerName])).entries()].sort((a, b) => a[1].localeCompare(b[1])));
const filtered = computed(() => {
  const query = search.value.trim().toLowerCase();
  return entries.value.filter((entry) => (provider.value === "all" || entry.providerId === provider.value) && (!query || `${entry.modelId} ${entry.modelName} ${entry.providerName}`.toLowerCase().includes(query)));
});
const selectedCount = computed(() => entries.value.filter((entry) => selectedKeys.value.has(entry.key)).length);

async function reloadConfig() {
  try { await loadModelsDevConfig(); } catch (cause) { Message.error(String(cause)); }
}
async function fetchEntries(force = false) {
  loading.value = true; error.value = "";
  try { entries.value = await fetchModelsDevEntries(force); } catch (cause) { error.value = String(cause); } finally { loading.value = false; }
}
async function openPicker(nextMode: "sync" | "import") {
  if (!config.value) await reloadConfig();
  mode.value = nextMode; search.value = ""; provider.value = "all";
  selected.value = nextMode === "sync" ? [...(config.value?.selectedModelKeys ?? [])] : [];
  excluded.value = nextMode === "sync" ? [...(config.value?.excludedCommonModelKeys ?? [])] : [];
  includeCommon.value = config.value?.includeCommonModels ?? true;
  visible.value = true;
  await fetchEntries();
}
function selectKeys(modelKeys: Iterable<string>, checked: boolean) {
  const keys = new Set(selected.value), omit = new Set(excluded.value);
  for (const key of modelKeys) {
    if (checked) { keys.add(key); omit.delete(key); } else { keys.delete(key); if (common.value.has(key)) omit.add(key); }
  }
  selected.value = [...keys]; excluded.value = [...omit];
}
function selectKey(key: string, checked: boolean) { selectKeys([key], checked); }
function selectFiltered(checked: boolean) { selectKeys(filtered.value.map((entry) => entry.key), checked); }
async function setEnabled(enabled: unknown) {
  if (!config.value) return;
  saving.value = true;
  try {
    await saveModelsDevConfig({ ...config.value, autoSyncEnabled: enabled === true });
    if (enabled === true) await syncModelsDevPricing();
  } catch (cause) { Message.error(String(cause)); } finally { saving.value = false; }
}
async function syncNow() {
  try { const changed = await syncModelsDevPricing(true); Message.success(`${t("同步完成")} · ${changed ?? 0} ${t("条规则")}`); } catch (cause) { Message.error(String(cause)); }
}
async function saveSelection() {
  saving.value = true;
  try {
    if (mode.value === "sync") {
      if (!config.value) throw new Error(t("同步配置尚未加载"));
      const next: ModelsDevSyncConfig = { ...config.value, includeCommonModels: includeCommon.value, selectedModelKeys: selected.value, excludedCommonModelKeys: excluded.value };
      await saveModelsDevConfig(next);
      Message.success(t("同步模型选择已保存"));
    } else {
      const changed = await importModelsDevPrices(toModelPricing(entries.value.filter((entry) => selectedKeys.value.has(entry.key))));
      Message.success(`${t("导入完成")} · ${changed} ${t("条规则")}`);
    }
    visible.value = false;
  } catch (cause) { Message.error(String(cause)); } finally { saving.value = false; }
}
async function openConfigFolder() {
  const path = modelsDevSyncState.value?.configPath;
  if (!path) return;
  try { await invoke("open_path_in_file_manager", { path: path.slice(0, Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"))) }); } catch (cause) { Message.error(String(cause)); }
}
onMounted(reloadConfig);
</script>

<template>
  <section class="usage-pricing-source-card usage-pricing-card">
    <div class="usage-pricing-card-head">
      <div class="usage-pricing-card-heading">
        <span class="usage-pricing-card-icon"><icon-public /></span>
        <strong>{{ t("价格数据源") }}</strong>
        <span class="usage-pricing-badge">models.dev</span>
      </div>
      <a-dropdown @select="(key: string | number | Record<string, unknown> | undefined) => key === 'reload' ? reloadConfig() : openConfigFolder()">
        <a-button type="text" size="mini" :aria-label="t('更多')"><icon-more /></a-button>
        <template #content>
          <div class="usage-sync-config-path" :title="modelsDevSyncState?.configPath">{{ modelsDevSyncState?.configPath }}</div>
          <a-doption value="folder">{{ t("打开配置文件夹") }}</a-doption>
          <a-doption value="reload">{{ t("重新读取本地配置") }}</a-doption>
        </template>
      </a-dropdown>
    </div>
    <p class="usage-pricing-description">{{ t("从模型目录导入单价，按需保持更新") }}</p>
    <div class="usage-sync-preference">
      <div>
        <strong>{{ t("自动同步模型价格") }}
          <a-tooltip :content="t('开启后在启动和每 6 小时同步所选模型价格，会覆盖这些模型的当前单价；未选模型保持原价。')"><icon-question-circle /></a-tooltip>
        </strong>
        <small>{{ config?.autoSyncEnabled ? t("已开启") : t("已关闭") }}<template v-if="config?.includeCommonModels"> · {{ t("含常用模型") }}</template></small>
      </div>
      <a-switch size="small" :model-value="config?.autoSyncEnabled ?? false" :loading="saving" :disabled="!config || modelsDevSyncing" :aria-label="t('自动同步模型价格')" @change="setEnabled" />
    </div>
    <div class="usage-sync-status">
      <span>{{ t("上次同步") }}：{{ config?.lastSyncAt ? formatAppDateTime(config.lastSyncAt) : t("尚未同步") }}</span>
      <a-button type="text" size="mini" :disabled="!config || modelsDevSyncing" @click="openPicker('sync')">{{ t("选择模型") }}<icon-right /></a-button>
    </div>
    <a-alert v-if="config?.lastSyncError" type="error" class="usage-sync-error">{{ config.lastSyncError }}</a-alert>
    <div class="usage-pricing-card-footer usage-sync-actions">
      <a-button size="small" @click="openPicker('import')"><template #icon><icon-import /></template>{{ t("从 models.dev 导入") }}</a-button>
      <a-button size="small" :loading="modelsDevSyncing" :disabled="saving || !config" @click="syncNow"><template #icon><icon-refresh /></template>{{ t("立即同步") }}</a-button>
    </div>
  </section>

  <a-modal v-model:visible="visible" :title="mode === 'sync' ? t('选择自动同步模型') : t('从 models.dev 导入')" width="min(1040px, 94vw)" :footer="false" :mask-closable="!saving" :esc-to-close="!saving" :closable="!saving" unmount-on-close>
    <a-alert>{{ t("价格单位为 USD / 每百万 Tokens。导入或同步会覆盖同名模型价格，同名模型按列表顺序保留首项。") }}</a-alert>
    <div class="usage-model-picker-toolbar">
      <a-input-search v-model="search" :placeholder="t('搜索模型 ID 或名称')" allow-clear />
      <a-select v-model="provider" :options="[{ value: 'all', label: t('全部供应商') }, ...providers.map(([value, label]) => ({value, label}))]" allow-search />
      <a-button :loading="loading" @click="fetchEntries(true)"><template #icon><icon-refresh /></template>{{ t("刷新") }}</a-button>
    </div>
    <div class="usage-model-picker-toolbar">
      <a-checkbox v-if="mode === 'sync'" v-model="includeCommon">{{ t("包含常用模型（每个系列最多 6 个）") }}</a-checkbox>
      <a-button size="small" @click="selectFiltered(true)">{{ t("全选筛选结果") }}</a-button>
      <a-button size="small" @click="selectFiltered(false)">{{ t("清空筛选结果") }}</a-button>
      <span>{{ t("已选择") }} {{ selectedCount }} / {{ entries.length }}</span>
    </div>
    <a-alert v-if="error" type="error">{{ error }}</a-alert>
    <a-table :data="filtered" :loading="loading" row-key="key" :pagination="{pageSize: 10, showTotal: true, showPageSize: true}" :scroll="{ y: 360 }" :bordered="false" class="usage-data-table">
      <template #columns>
        <a-table-column :width="44"><template #cell="{record}"><a-checkbox :model-value="selectedKeys.has(record.key)" :aria-label="record.providerName + ' ' + record.modelId" @change="(checked: boolean | (string | number | boolean)[]) => selectKey(record.key, checked === true)" /></template></a-table-column>
        <a-table-column :title="t('供应商')" data-index="providerName" :width="140" />
        <a-table-column :title="t('模型')" :width="330"><template #cell="{record}"><div class="usage-price-model"><strong>{{ record.modelName }}</strong><code>{{ record.modelId }}</code><small v-if="common.has(record.key)">{{ t("常用模型") }}</small></div></template></a-table-column>
        <a-table-column :title="t('输入单价')" align="right"><template #cell="{record}">{{ formatUnitPrice(String(record.input)) }}</template></a-table-column>
        <a-table-column :title="t('输出单价')" align="right"><template #cell="{record}">{{ formatUnitPrice(String(record.output)) }}</template></a-table-column>
        <a-table-column :title="t('缓存命中')" align="right"><template #cell="{record}">{{ formatUnitPrice(String(record.cacheRead)) }}</template></a-table-column>
        <a-table-column :title="t('缓存写入')" align="right"><template #cell="{record}">{{ formatUnitPrice(String(record.cacheWrite)) }}</template></a-table-column>
      </template>
    </a-table>
    <div class="usage-pricing-modal-actions"><a-button :disabled="saving" @click="visible = false">{{ t("取消") }}</a-button><a-button type="primary" :loading="saving" :disabled="loading || !!error || (mode === 'import' && selectedCount === 0)" @click="saveSelection">{{ mode === 'sync' ? t("保存选择") : t("导入所选模型") }}</a-button></div>
  </a-modal>
</template>
