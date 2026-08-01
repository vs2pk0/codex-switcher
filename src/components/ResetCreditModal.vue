<script setup lang="ts">
import { formatLocalizedCount, t } from "../i18n";
import { resetCreditRowActionState } from "../services/resetUiState";
import type { CodexAccount, CodexResetCredit } from "../types/codex";
import type { ScheduledReset } from "../types/reset";

const props = defineProps<{
  visible: boolean;
  account: CodexAccount | null;
  records: CodexResetCredit[];
  quotaRefreshingId: string;
  displayName: (account: CodexAccount) => string;
  resetCreditCount: (account: CodexAccount) => number;
  isAvailableResetCredit: (credit: CodexResetCredit) => boolean;
  resetCreditStatusKey: (credit: CodexResetCredit) => "available" | "used" | "expired" | "unknown";
  resetCreditStatusLabel: (credit: CodexResetCredit) => string;
  formatResetCreditDate: (value?: number) => string;
  scheduledReset: ScheduledReset | null;
  resetStateBusy: boolean;
}>();

const emit = defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "consume"): void;
  (event: "open-schedule"): void;
  (event: "view-schedules"): void;
}>();

function rowActionState(credit: CodexResetCredit) {
  return resetCreditRowActionState(
    props.isAvailableResetCredit(credit),
    Boolean(props.scheduledReset),
    props.resetStateBusy || props.quotaRefreshingId === props.account?.id,
  );
}

function openScheduleFromRow(credit: CodexResetCredit): void {
  const state = rowActionState(credit);
  if (state.scheduleDisabled) return;
  if (state.scheduleAction === "view") {
    emit("view-schedules");
  } else {
    emit("open-schedule");
  }
}
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('重置次数明细')"
    :footer="false"
    width="860px"
    modal-class="reset-credit-modal"
    @update:visible="$emit('update:visible', $event)"
  >
    <div v-if="account" class="reset-credit-modal-body">
      <div class="reset-credit-modal-head">
        <div>
          <span class="modal-eyebrow">{{ t("重置次数") }}</span>
          <h3>{{ t("可用重置次数明细") }}</h3>
          <p>
            {{ displayName(account) }} {{ t("当前有") }}
            {{ formatLocalizedCount(resetCreditCount(account), "次") }} {{ t("可用重置次数") }}。
          </p>
        </div>
        <div class="reset-credit-modal-count">
          <strong>{{ resetCreditCount(account) }}</strong>
          <span>{{ t("次可用") }}</span>
        </div>
      </div>

      <div class="reset-credit-account-action-note">
        <icon-info-circle />
        <span>{{ t("按账号执行，实际消耗记录由服务端决定") }}</span>
      </div>

      <div v-if="records.length" class="reset-credit-choice-list">
        <article
          v-for="(credit, index) in records"
          :key="credit.id || `${account.id}-modal-credit-${index}`"
          class="reset-credit-choice"
          :class="{ disabled: !isAvailableResetCredit(credit) }"
        >
          <span class="reset-credit-choice-status" :class="`is-${resetCreditStatusKey(credit)}`">
            {{ resetCreditStatusLabel(credit) }}
          </span>
          <span class="reset-credit-choice-main">
            <strong>{{ t(`第 ${index + 1} 次`) }}</strong>
            <small>{{ t("发放") }} {{ formatResetCreditDate(credit.granted_at) }}</small>
          </span>
          <span class="reset-credit-choice-time">
            {{ t("可用至") }} {{ formatResetCreditDate(credit.expires_at) }}
          </span>
          <span class="reset-credit-choice-actions">
            <a-button
              size="small"
              type="primary"
              status="warning"
              :loading="quotaRefreshingId === account.id"
              :disabled="rowActionState(credit).consumeDisabled"
              @click="emit('consume')"
            >
              <template #icon><icon-thunderbolt /></template>
              {{ t("立即重置") }}
            </a-button>
            <a-button
              size="small"
              type="outline"
              :class="{ 'reset-credit-scheduled-button': Boolean(scheduledReset) }"
              :loading="!scheduledReset && resetStateBusy"
              :disabled="rowActionState(credit).scheduleDisabled"
              @click="openScheduleFromRow(credit)"
            >
              <template #icon><icon-calendar /></template>
              {{ scheduledReset ? t("已预约") : t("预约重置") }}
            </a-button>
          </span>
        </article>
      </div>
      <a-empty v-else :description="t('暂无重置次数明细，请先刷新额度')" />

      <div class="reset-credit-modal-actions">
        <a-button @click="$emit('update:visible', false)">{{ t("关闭") }}</a-button>
      </div>
    </div>
  </a-modal>
</template>
