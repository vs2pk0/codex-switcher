<script setup lang="ts">
import { computed } from "vue";
import { currentLocale, t } from "../i18n";
import { formatResetCountdown, type ResetState, type ScheduledReset } from "../services/reset";
import { formatResetDateTime } from "../services/resetUiState";
import type { CodexAccount } from "../types/codex";

const props = defineProps<{
  state: ResetState;
  accounts: CodexAccount[];
  nowMs: number;
  loading: boolean;
  saving: boolean;
  updatingScheduleIds: string[];
  cancellingScheduleIds: string[];
  deletingLogIds: string[];
  clearingLogs: boolean;
}>();

const emit = defineEmits<{
  (event: "refresh"): void;
  (event: "edit-schedule", task: ScheduledReset): void;
  (event: "cancel-schedule", scheduleId: string): void;
  (event: "delete-log", logId: string): void;
  (event: "clear-logs"): void;
}>();

const activeSchedules = computed(() =>
  props.state.scheduledResets
    .filter((task) => task.status === "scheduled" || task.status === "running")
    .sort((left, right) => left.scheduledAt - right.scheduledAt),
);

const logs = computed(() =>
  [...props.state.logs].sort((left, right) => right.occurredAt - left.occurredAt),
);

function formatDateTime(timestamp: number): string {
  return formatResetDateTime(timestamp, currentLocale());
}

function countdown(task: ScheduledReset): string {
  if (task.status === "running") return t("执行中");
  return formatResetCountdown(task.scheduledAt - props.nowMs);
}

function statusLabel(status: ScheduledReset["status"]): string {
  const labels: Record<ScheduledReset["status"], string> = {
    scheduled: "已预约",
    running: "执行中",
    completed: "已完成",
    failed: "失败",
    missed: "未执行",
    cancelled: "已取消",
  };
  return t(labels[status]);
}

function resultLabel(result: ResetState["logs"][number]["result"]): string {
  const labels: Record<ResetState["logs"][number]["result"], string> = {
    success: "成功",
    failed: "失败",
    missed: "未执行",
    cancelled: "已取消",
  };
  return t(labels[result]);
}

function typeLabel(type: ResetState["logs"][number]["type"]): string {
  return t(type === "immediate" ? "立即重置" : "预约重置");
}
</script>

<template>
  <section class="reset-panel">
    <header class="reset-panel-head">
      <div>
        <span class="panel-eyebrow">{{ t("额度操作") }}</span>
        <h2>{{ t("重置记录") }}</h2>
        <p>{{ t("查看预约重置倒计时和历史执行结果") }}</p>
      </div>
      <a-button :loading="loading || saving" @click="emit('refresh')">
        <template #icon><icon-refresh /></template>
        {{ t("刷新") }}
      </a-button>
    </header>

    <section class="reset-panel-section">
      <div class="reset-panel-section-head">
        <h3>{{ t("活动预约") }}</h3>
        <span>{{ activeSchedules.length }} {{ t("条") }}</span>
      </div>
      <div v-if="activeSchedules.length" class="reset-schedule-list">
        <article v-for="task in activeSchedules" :key="task.id" class="reset-schedule-row">
          <div class="reset-schedule-main">
            <strong>{{ task.accountLabel }}</strong>
            <span>{{ formatDateTime(task.scheduledAt) }}</span>
          </div>
          <div class="reset-schedule-countdown">
            <small>{{ statusLabel(task.status) }}</small>
            <b>{{ countdown(task) }}</b>
          </div>
          <div v-if="task.status === 'scheduled'" class="reset-schedule-actions">
            <a-button
              type="text"
              :title="t('修改预约时间')"
              :loading="updatingScheduleIds.includes(task.id)"
              :disabled="saving || cancellingScheduleIds.includes(task.id)"
              @click="emit('edit-schedule', task)"
            >
              <template #icon><icon-edit /></template>
            </a-button>
            <a-popconfirm
              :content="t('确认取消该预约？')"
              :ok-text="t('确认')"
              :cancel-text="t('取消')"
              :disabled="saving || updatingScheduleIds.includes(task.id)"
              @ok="emit('cancel-schedule', task.id)"
            >
              <a-button
                type="text"
                status="danger"
                :title="t('取消预约')"
                :loading="cancellingScheduleIds.includes(task.id)"
                :disabled="saving || updatingScheduleIds.includes(task.id)"
              >
                <template #icon><icon-close-circle /></template>
              </a-button>
            </a-popconfirm>
          </div>
        </article>
      </div>
      <a-empty v-else :description="t('暂无活动预约')" />
    </section>

    <section class="reset-panel-section">
      <div class="reset-panel-section-head">
        <h3>{{ t("重置日志") }}</h3>
        <div class="reset-panel-section-tools">
          <span>{{ logs.length }} {{ t("条") }}</span>
          <a-popconfirm
            :content="t('确认清空全部重置日志？此操作不可恢复。')"
            :ok-text="t('确认')"
            :cancel-text="t('取消')"
            :disabled="!logs.length || clearingLogs || deletingLogIds.length > 0 || saving"
            @ok="emit('clear-logs')"
          >
            <a-button
              size="small"
              status="danger"
              :loading="clearingLogs"
              :disabled="!logs.length || deletingLogIds.length > 0 || saving"
            >
              <template #icon><icon-delete /></template>
              {{ t("清空日志") }}
            </a-button>
          </a-popconfirm>
        </div>
      </div>
      <div v-if="logs.length" class="reset-log-list">
        <article v-for="log in logs" :key="log.id" class="reset-log-row">
          <div class="reset-log-main">
            <strong>{{ log.accountLabel }}</strong>
            <span>{{ formatDateTime(log.occurredAt) }}</span>
          </div>
          <span class="reset-log-type">{{ typeLabel(log.type) }}</span>
          <span class="reset-log-result" :class="`is-${log.result}`">
            {{ resultLabel(log.result) }}
          </span>
          <span class="reset-log-error">{{ log.error || "" }}</span>
          <a-popconfirm
            :content="t('确认删除该条重置日志？')"
            :ok-text="t('确认')"
            :cancel-text="t('取消')"
            :disabled="clearingLogs || saving"
            @ok="emit('delete-log', log.id)"
          >
            <a-button
              type="text"
              status="danger"
              :title="t('删除日志')"
              :loading="deletingLogIds.includes(log.id)"
              :disabled="clearingLogs || saving"
            >
              <template #icon><icon-delete /></template>
            </a-button>
          </a-popconfirm>
        </article>
      </div>
      <a-empty v-else :description="t('暂无重置日志')" />
    </section>
  </section>
</template>
