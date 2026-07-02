<script setup lang="ts">
import { formatLocalizedCount, t } from "../i18n";
import type { CodexAccount, CodexResetCredit } from "../types/codex";

defineProps<{
  visible: boolean;
  account: CodexAccount | null;
  records: CodexResetCredit[];
  selectedIndex: number;
  selectedCredit: CodexResetCredit | null;
  quotaRefreshingId: string;
  displayName: (account: CodexAccount) => string;
  resetCreditCount: (account: CodexAccount) => number;
  isAvailableResetCredit: (credit: CodexResetCredit) => boolean;
  resetCreditStatusKey: (credit: CodexResetCredit) => "available" | "used" | "expired" | "unknown";
  resetCreditStatusLabel: (credit: CodexResetCredit) => string;
  formatResetCreditDate: (value?: number) => string;
}>();

defineEmits<{
  (event: "update:visible", value: boolean): void;
  (event: "update:selected-index", value: number): void;
  (event: "consume"): void;
}>();
</script>

<template>
  <a-modal
    :visible="visible"
    :title="t('选择重置次数')"
    :footer="false"
    width="680px"
    modal-class="reset-credit-modal"
    @update:visible="$emit('update:visible', $event)"
  >
    <div v-if="account" class="reset-credit-modal-body">
      <div class="reset-credit-modal-head">
        <div>
          <span class="modal-eyebrow">{{ t("重置次数") }}</span>
          <h3>{{ t("选择要消耗的重置次数") }}</h3>
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

      <div v-if="records.length" class="reset-credit-choice-list">
        <button
          v-for="(credit, index) in records"
          :key="credit.id || `${account.id}-modal-credit-${index}`"
          class="reset-credit-choice"
          :class="{
            selected: selectedIndex === index,
            disabled: !isAvailableResetCredit(credit),
          }"
          type="button"
          :disabled="!isAvailableResetCredit(credit)"
          @click="$emit('update:selected-index', index)"
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
        </button>
      </div>
      <a-empty v-else :description="t('暂无重置次数明细，请先刷新额度')" />

      <div class="reset-credit-modal-actions">
        <a-button @click="$emit('update:visible', false)">{{ t("取消") }}</a-button>
        <a-button
          type="primary"
          status="warning"
          :loading="quotaRefreshingId === account.id"
          :disabled="!selectedCredit || !isAvailableResetCredit(selectedCredit)"
          @click="$emit('consume')"
        >
          <template #icon><icon-thunderbolt /></template>
          {{ t("重置使用次数") }}
        </a-button>
      </div>
    </div>
  </a-modal>
</template>
