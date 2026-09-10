<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Message, Modal } from "@arco-design/web-vue";
import {
  deleteSessionEditBackups,
  listSessionEditBackups,
  openPathInFileManager,
  type SessionEditBackupDeleteScope,
  type SessionEditBackupSummary,
} from "../services/session";
import { formatTranslatedText, t } from "../i18n";

const props = defineProps<{
  instanceId: string;
  /** 会话列表刷新完成后（true→false）重新统计备份。 */
  sessionLoading: boolean;
  disabled?: boolean;
}>();

const summary = ref<SessionEditBackupSummary | null>(null);
const loading = ref(false);
const deleting = ref(false);
const managerVisible = ref(false);
const selected = ref<Set<string>>(new Set());

/** 文件名中的操作类型 → 展示文案。 */
const OPERATION_LABELS: Record<string, string> = {
  "turn-delete": "删除轮次",
  "message-delete": "删除消息",
  "before-restore": "恢复前快照",
  cwd: "修改工作目录",
  "model-compatibility": "切号会话修复",
};

const selectedCount = computed(() => selected.value.size);
const allSelected = computed(() =>
  Boolean(summary.value?.entries.length) && summary.value!.entries.every((entry) => selected.value.has(entry.fileName)),
);

function formatFileSize(bytes?: number | null): string {
  const safeBytes = typeof bytes === "number" && Number.isFinite(bytes) ? bytes : 0;
  if (safeBytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = safeBytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const digits = value >= 10 || unitIndex === 0 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unitIndex]}`;
}

function formatTime(value: string | null): string {
  if (!value) return "-";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "-" : date.toLocaleString();
}

function operationLabel(operation: string): string {
  const label = OPERATION_LABELS[operation];
  return label ? t(label) : operation;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function load(): Promise<void> {
  loading.value = true;
  const instanceId = props.instanceId;
  try {
    const result = await listSessionEditBackups(instanceId);
    if (instanceId !== props.instanceId) return;
    summary.value = result;
    // 只保留仍存在的选中项。
    const existing = new Set(result.entries.map((entry) => entry.fileName));
    selected.value = new Set([...selected.value].filter((name) => existing.has(name)));
  } catch (error) {
    if (instanceId !== props.instanceId) return;
    summary.value = null;
    Message.error(formatTranslatedText("读取会话编辑备份失败：{error}", { error: errorText(error) }));
  } finally {
    loading.value = false;
  }
}

function toggle(fileName: string): void {
  const next = new Set(selected.value);
  if (next.has(fileName)) next.delete(fileName);
  else next.add(fileName);
  selected.value = next;
}

function toggleAll(): void {
  selected.value = allSelected.value
    ? new Set()
    : new Set((summary.value?.entries ?? []).map((entry) => entry.fileName));
}

async function runDelete(scope: SessionEditBackupDeleteScope): Promise<void> {
  deleting.value = true;
  try {
    const result = await deleteSessionEditBackups(props.instanceId, scope, scope === "selected" ? [...selected.value] : []);
    summary.value = result.summary;
    selected.value = new Set();
    Message.success(formatTranslatedText("已清理 {count} 个备份，释放 {size}", {
      count: String(result.deletedCount),
      size: formatFileSize(result.deletedBytes),
    }));
  } catch (error) {
    Message.error(formatTranslatedText("清理会话编辑备份失败：{error}", { error: errorText(error) }));
  } finally {
    deleting.value = false;
  }
}

function confirmDelete(scope: SessionEditBackupDeleteScope): void {
  const current = summary.value;
  if (!current) return;
  const [count, bytes] = scope === "selected"
    ? [selectedCount.value, current.entries.filter((entry) => selected.value.has(entry.fileName)).reduce((sum, entry) => sum + entry.sizeBytes, 0)]
    : scope === "instance"
      ? [current.instanceCount, current.instanceBytes]
      : scope === "orphan"
        ? [current.orphanCount, current.orphanBytes]
        : [current.totalCount, current.totalBytes];
  if (!count) return;
  const titles: Record<SessionEditBackupDeleteScope, string> = {
    selected: "删除选中的备份",
    instance: "清空当前实例的备份",
    orphan: "清理无法归属的备份",
    all: "清空全部实例的备份",
  };
  Modal.confirm({
    title: t(titles[scope]),
    content: formatTranslatedText("将永久删除 {count} 个备份文件（{size}）。这些备份用于撤销会话编辑，删除后对应操作将无法回退。", {
      count: String(count),
      size: formatFileSize(bytes),
    }),
    okText: t("删除"),
    cancelText: t("取消"),
    okButtonProps: { status: "danger" },
    onOk: () => runDelete(scope),
  });
}

function openManager(): void {
  managerVisible.value = true;
  void load();
}

watch(() => props.instanceId, () => { selected.value = new Set(); void load(); }, { immediate: true });
watch(() => props.sessionLoading, (value, previous) => {
  if (previous && !value) void load();
});
</script>

<template>
  <article class="session-backup-card" :class="{ clickable: !disabled }" role="button" tabindex="0" @click="!disabled && openManager()" @keydown.enter.prevent="!disabled && openManager()">
    <span class="session-overview-icon rose"><icon-history /></span>
    <div>
      <small>{{ t("编辑备份") }}</small>
      <strong>
        {{ formatFileSize(summary?.instanceBytes) }}
        <em>{{ formatTranslatedText("{count} 个", { count: String(summary?.instanceCount ?? 0) }) }}</em>
      </strong>
      <!-- 当前实例之外还有备份时，提示全部实例合计，避免误以为目录为空。 -->
      <small v-if="summary && summary.totalCount > summary.instanceCount" class="session-backup-card-total">
        {{ formatTranslatedText("全部实例合计 {size} · {count} 个", { size: formatFileSize(summary.totalBytes), count: String(summary.totalCount) }) }}
      </small>
    </div>
    <span class="session-backup-card-action"><icon-settings />{{ t("管理") }}</span>
  </article>

  <a-modal
    v-model:visible="managerVisible"
    :title="t('会话编辑备份')"
    :footer="false"
    :width="720"
    :mask-closable="!deleting"
    :closable="!deleting"
    unmount-on-close
  >
    <div class="session-backup-manager">
      <p class="session-backup-intro">
        {{ t("删除轮次、删除消息、修改工作目录和一键修复切号会话前，都会先把原会话文件备份到这里，以便撤销。备份不会自动清理，可在此按实例查看并释放空间。") }}
      </p>
      <div class="session-backup-stats">
        <div class="current">
          <small>{{ t("当前实例") }}</small>
          <strong>{{ formatFileSize(summary?.instanceBytes) }}</strong>
          <span>{{ formatTranslatedText("{count} 个", { count: String(summary?.instanceCount ?? 0) }) }}</span>
        </div>
        <div>
          <small>{{ t("其他实例") }}</small>
          <strong>{{ formatFileSize(summary?.otherInstanceBytes) }}</strong>
          <span>{{ formatTranslatedText("{count} 个", { count: String(summary?.otherInstanceCount ?? 0) }) }}</span>
        </div>
        <div :class="{ warning: summary?.orphanCount }">
          <small>{{ t("无法归属") }}</small>
          <strong>{{ formatFileSize(summary?.orphanBytes) }}</strong>
          <span>{{ formatTranslatedText("{count} 个", { count: String(summary?.orphanCount ?? 0) }) }}</span>
        </div>
        <div>
          <small>{{ t("合计") }}</small>
          <strong>{{ formatFileSize(summary?.totalBytes) }}</strong>
          <span>{{ formatTranslatedText("{count} 个", { count: String(summary?.totalCount ?? 0) }) }}</span>
        </div>
      </div>
      <div class="session-backup-toolbar">
        <a-button size="small" :loading="loading" :disabled="deleting" @click="load"><template #icon><icon-refresh /></template>{{ t("刷新") }}</a-button>
        <a-button size="small" :disabled="!summary?.entries.length || deleting" @click="toggleAll"><template #icon><icon-check /></template>{{ t(allSelected ? "取消全选" : "全选") }}</a-button>
        <a-button size="small" :disabled="!summary" @click="summary && openPathInFileManager(summary.directory)"><template #icon><icon-folder /></template>{{ t("打开目录") }}</a-button>
        <span class="session-backup-toolbar-spacer" />
        <a-button size="small" status="danger" :disabled="!selectedCount || deleting" @click="confirmDelete('selected')"><template #icon><icon-delete /></template>{{ formatTranslatedText("删除选中（{count}）", { count: String(selectedCount) }) }}</a-button>
        <a-button size="small" status="danger" :disabled="!summary?.instanceCount || deleting" @click="confirmDelete('instance')">{{ t("清空当前实例") }}</a-button>
        <a-button size="small" status="warning" :disabled="!summary?.orphanCount || deleting" @click="confirmDelete('orphan')">{{ t("清理无法归属") }}</a-button>
        <a-button size="small" status="danger" type="outline" :disabled="!summary?.totalCount || deleting" @click="confirmDelete('all')">{{ t("清空全部实例") }}</a-button>
      </div>
      <a-spin :loading="loading || deleting" class="session-backup-list-spin">
        <div v-if="summary?.entries.length" class="session-backup-list">
          <label v-for="entry in summary.entries" :key="entry.fileName" class="session-backup-row" :class="{ selected: selected.has(entry.fileName) }">
            <a-checkbox :model-value="selected.has(entry.fileName)" :disabled="deleting" @change="toggle(entry.fileName)" />
            <span class="session-backup-op">{{ operationLabel(entry.operation) }}</span>
            <span class="session-backup-time">{{ formatTime(entry.createdAt) }}</span>
            <span class="session-backup-size">{{ formatFileSize(entry.sizeBytes) }}</span>
            <code class="session-backup-file" :title="entry.fileName">{{ entry.fileName }}</code>
          </label>
        </div>
        <a-empty v-else :description="t('当前实例没有会话编辑备份')" />
      </a-spin>
      <p v-if="summary?.orphanCount" class="session-backup-hint">
        {{ t("「无法归属」的备份对应的会话已在所有实例中彻底删除，通常可以安全清理。") }}
      </p>
    </div>
  </a-modal>
</template>
