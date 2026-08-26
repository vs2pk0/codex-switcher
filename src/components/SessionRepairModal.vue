<script setup lang="ts">
import { t } from "../i18n";
import type {
  CodexSessionVisibilityRepairInstanceOption,
  CodexSessionVisibilityRepairMode,
  CodexSessionVisibilityRepairSummary,
} from "../services/session";

defineProps<{
  visible: boolean;
  selectedCount: number;
  totalCount: number;
  repairMode: CodexSessionVisibilityRepairMode;
  repairTargetInstanceId: string;
  repairInstances: CodexSessionVisibilityRepairInstanceOption[];
  repairInstanceScope: "target" | "all";
  repairSessionScope: "all" | "selected";
  effectiveRepairSessionScope: "all" | "selected";
  repairResult: CodexSessionVisibilityRepairSummary | null;
  sessionRepairing: boolean;
}>();

defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "update:repair-mode", value: CodexSessionVisibilityRepairMode): void;
  (event: "update:repair-target-instance-id", value: string): void;
  (event: "update:repair-instance-scope", value: "target" | "all"): void;
  (event: "update:repair-session-scope", value: "all" | "selected"): void;
  (event: "run"): void;
}>();
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('找回会话显示')"
    :footer="false"
    width="900px"
    modal-class="repair-modal"
    @update:visible="$emit('update:visible', $event)"
  >
    <div class="repair-body">
      <div class="repair-hero">
        <div>
          <span class="modal-eyebrow">{{ t("会话恢复") }}</span>
          <h3>{{ t("把切号后消失的会话重新挂回列表") }}</h3>
          <p>
            {{ t("会同步整理 Codex 本地索引、项目分组、图片附件与状态库，并自动重启 ChatGPT/Codex 让当前侧栏重新加载；请先等待正在运行的任务结束。") }}
          </p>
        </div>
        <div class="repair-summary-card">
          <strong>{{ selectedCount || totalCount }}</strong>
          <span>{{ selectedCount ? t("条已选会话") : t("条可处理会话") }}</span>
        </div>
      </div>

      <div class="repair-section repair-section-inline">
        <div class="repair-section-copy">
          <span class="repair-section-title">{{ t("处理强度") }}</span>
          <small>{{ t("优先使用轻量模式；仍然看不到再切到完整重建。") }}</small>
        </div>
        <div class="repair-card-grid">
          <button
            class="repair-option-card"
            :class="{ selected: repairMode === 'quick' }"
            type="button"
            @click="$emit('update:repair-mode', 'quick')"
          >
            <strong>{{ t("轻量同步") }}</strong>
            <small>{{ t("更新状态库并补齐缺失记录，速度更快。") }}</small>
          </button>
          <button
            class="repair-option-card"
            :class="{ selected: repairMode === 'deep' }"
            type="button"
            @click="$emit('update:repair-mode', 'deep')"
          >
            <strong>{{ t("完整重建") }}</strong>
            <small>{{ t("额外重写 session_index，适合普通同步无效时。") }}</small>
          </button>
        </div>
      </div>

      <div class="repair-control-panel">
        <div class="repair-section">
          <span class="repair-section-title">{{ t("Codex 实例") }}</span>
          <a-select
            :model-value="repairTargetInstanceId"
            :placeholder="t('默认实例')"
            @change="$emit('update:repair-target-instance-id', String($event))"
          >
            <a-option
              v-for="instance in repairInstances"
              :key="instance.id"
              :value="instance.id"
            >
              {{ instance.name }} · {{ instance.currentProvider }}
            </a-option>
          </a-select>
        </div>
        <div class="repair-section">
          <span class="repair-section-title">{{ t("实例覆盖") }}</span>
          <div class="repair-segmented">
            <button
              :class="{ selected: repairInstanceScope === 'target' }"
              type="button"
              @click="$emit('update:repair-instance-scope', 'target')"
            >
              {{ t("当前实例") }}
            </button>
            <button
              :class="{ selected: repairInstanceScope === 'all' }"
              type="button"
              @click="$emit('update:repair-instance-scope', 'all')"
            >
              {{ t("本机全部") }}
            </button>
          </div>
        </div>
        <div class="repair-section">
          <span class="repair-section-title">{{ t("会话覆盖") }}</span>
          <div class="repair-segmented">
            <button
              :class="{ selected: effectiveRepairSessionScope === 'all' }"
              type="button"
              @click="$emit('update:repair-session-scope', 'all')"
            >
              {{ t("全部") }} {{ totalCount }}
            </button>
            <button
              :class="{ selected: effectiveRepairSessionScope === 'selected' }"
              :disabled="!selectedCount"
              type="button"
              @click="$emit('update:repair-session-scope', 'selected')"
            >
              {{ t("已选") }} {{ selectedCount }}
            </button>
          </div>
        </div>
      </div>

      <div v-if="repairResult" class="repair-result">
        <strong>{{ repairResult.message }}</strong>
        <span v-if="repairResult.changedRolloutFileCount !== undefined">
          {{ t("会话文件") }} {{ repairResult.changedRolloutFileCount }} {{ t("个") }}
        </span>
        <span>SQLite {{ repairResult.updatedSqliteRowCount ?? repairResult.repaired }} {{ t("条") }}</span>
        <span v-if="repairResult.updatedSqliteTimestampRowCount">
          {{ t("时间记录") }} {{ repairResult.updatedSqliteTimestampRowCount }} {{ t("条") }}
        </span>
        <span v-if="repairResult.addedSessionIndexEntryCount">
          session_index {{ repairResult.addedSessionIndexEntryCount }} {{ t("条") }}
        </span>
        <span v-if="repairResult.updatedCatalogRowCount !== undefined">
          {{ t("侧栏目录") }} {{ repairResult.updatedCatalogRowCount }} {{ t("条") }}
        </span>
        <span v-if="repairResult.verifiedVisibleSessionCount !== undefined">
          {{ t("目录校验") }} {{ repairResult.verifiedVisibleSessionCount }} {{ t("条") }}
        </span>
        <span v-if="repairResult.skippedNonSidebarSessionCount">
          {{ t("跳过子代理") }} {{ repairResult.skippedNonSidebarSessionCount }} {{ t("条") }}
        </span>
        <span v-if="repairResult.createdLocalProjectCount !== undefined">
          {{ t("新建项目") }} {{ repairResult.createdLocalProjectCount }} {{ t("个") }}
        </span>
        <span v-if="repairResult.assignedLocalProjectSessionCount !== undefined">
          {{ t("会话归组") }} {{ repairResult.assignedLocalProjectSessionCount }} {{ t("条") }}
        </span>
        <span v-if="repairResult.verifiedLocalProjectCount !== undefined">
          {{ t("项目校验") }} {{ repairResult.verifiedLocalProjectCount }} {{ t("个") }}
        </span>
        <span v-if="repairResult.recreatedGeneratedImageCount !== undefined">
          {{ t("重建图片") }} {{ repairResult.recreatedGeneratedImageCount }} {{ t("张") }}
        </span>
        <span v-if="repairResult.verifiedGeneratedImageCount !== undefined">
          {{ t("图片校验") }} {{ repairResult.verifiedGeneratedImageCount }} {{ t("张") }}
        </span>
        <span v-if="repairResult.invalidGeneratedImageCount" class="repair-result-error">
          {{ t("无效图片") }} {{ repairResult.invalidGeneratedImageCount }} {{ t("张") }}
        </span>
        <span v-if="repairResult.resetHistoryProjectionCount">
          {{ t("历史投影") }} {{ repairResult.resetHistoryProjectionCount }} {{ t("条") }}
        </span>
        <span v-if="repairResult.desktopReloadPerformed">
          {{ t("侧栏已重载") }}
        </span>
        <span v-if="repairResult.remainingInvisibleSessionCount" class="repair-result-error">
          {{ t("仍不可见") }} {{ repairResult.remainingInvisibleSessionCount }} {{ t("条") }}
        </span>
      </div>

      <div class="form-actions">
        <a-button @click="$emit('update:visible', false)">{{ t("关闭") }}</a-button>
        <a-button type="primary" :loading="sessionRepairing" @click="$emit('run')">
          <template #icon><icon-refresh /></template>
          {{ t("立即找回") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
